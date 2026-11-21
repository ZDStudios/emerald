//! CRX3 packages (Chrome Web Store / Edge Add-ons): verify, identify, unpack.
//!
//! Format: "Cr24" | u32 version (3) | u32 header_len | CrxFileHeader (protobuf) | ZIP.
//! The header holds RSA/ECDSA proofs over:
//!   "CRX3 SignedData\0" | u32 len(signed_header_data) | signed_header_data | ZIP
//! and `signed_header_data` (SignedData proto) holds the 16-byte `crx_id`.
//! The extension ID is the first 16 bytes of SHA-256(public key), written with a–p.
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::pkcs8::DecodePublicKey;
use rsa::signature::Verifier;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug)]
pub struct Crx {
    pub id: String,
    pub public_key: Vec<u8>,
    pub archive: Vec<u8>,
}

fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *b.get(*i)?;
        *i += 1;
        v |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 { return Some(v); }
    }
    None
}

/// Iterate (field number, bytes) of length-delimited fields in a protobuf message.
fn fields(b: &[u8]) -> Vec<(u64, &[u8])> {
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        let Some(key) = varint(b, &mut i) else { break };
        match key & 7 {
            0 => { if varint(b, &mut i).is_none() { break; } }
            1 => i += 8,
            5 => i += 4,
            2 => {
                let Some(len) = varint(b, &mut i) else { break };
                let end = i + len as usize;
                if end > b.len() { break; }
                out.push((key >> 3, &b[i..end]));
                i = end;
            }
            _ => break,
        }
    }
    out
}

pub fn id_from_key(public_key: &[u8]) -> String {
    let h = Sha256::digest(public_key);
    h[..16].iter().flat_map(|b| [b >> 4, b & 0xf]).map(|n| (b'a' + n) as char).collect()
}

/// Parse and verify a CRX3. `expected_id`, when given, must match.
pub fn parse(data: &[u8], expected_id: Option<&str>) -> anyhow::Result<Crx> {
    anyhow::ensure!(data.len() > 12 && &data[..4] == b"Cr24", "not a CRX file");
    let version = u32::from_le_bytes(data[4..8].try_into()?);
    anyhow::ensure!(version == 3, "unsupported CRX version {version} (only CRX3)");
    let hlen = u32::from_le_bytes(data[8..12].try_into()?) as usize;
    anyhow::ensure!(data.len() >= 12 + hlen, "truncated CRX header");
    let header = &data[12..12 + hlen];
    let archive = &data[12 + hlen..];
    let hf = fields(header);
    let signed = hf.iter().find(|(n, _)| *n == 10000).map(|x| x.1).ok_or_else(|| anyhow::anyhow!("CRX has no signed header data"))?;
    let crx_id = fields(signed).into_iter().find(|(n, _)| *n == 1).map(|x| x.1).ok_or_else(|| anyhow::anyhow!("CRX has no id"))?;
    anyhow::ensure!(crx_id.len() == 16, "bad crx id length");
    let id: String = crx_id.iter().flat_map(|b| [b >> 4, b & 0xf]).map(|n| (b'a' + n) as char).collect();
    if let Some(e) = expected_id { anyhow::ensure!(e == id, "CRX id {id} does not match requested {e}"); }

    let mut msg = b"CRX3 SignedData\x00".to_vec();
    msg.extend((signed.len() as u32).to_le_bytes());
    msg.extend(signed);
    msg.extend(archive);

    // Find the RSA proof whose key hashes to the id, and verify its signature.
    let mut key_for_id = None;
    for (n, proof) in &hf {
        if *n != 2 { continue; } // sha256_with_rsa
        let pf = fields(proof);
        let (Some(pk), Some(sig)) = (pf.iter().find(|x| x.0 == 1).map(|x| x.1), pf.iter().find(|x| x.0 == 2).map(|x| x.1)) else { continue };
        if id_from_key(pk) != id { continue; }
        let key = rsa::RsaPublicKey::from_public_key_der(pk).map_err(|e| anyhow::anyhow!("bad public key: {e}"))?;
        let vk = VerifyingKey::<Sha256>::new(key);
        let sig = Signature::try_from(sig).map_err(|e| anyhow::anyhow!("bad signature: {e}"))?;
        vk.verify(&msg, &sig).map_err(|_| anyhow::anyhow!("CRX signature does not verify"))?;
        key_for_id = Some(pk.to_vec());
    }
    let public_key = key_for_id.ok_or_else(|| anyhow::anyhow!("no developer signature matching the extension id"))?;
    Ok(Crx { id, public_key, archive: archive.to_vec() })
}

/// Unpack into `dir` (replacing it) and set `manifest.key` so Chromium derives the same ID.
pub fn unpack(crx: &Crx, dir: &Path) -> anyhow::Result<serde_json::Value> {
    let tmp = dir.with_extension("unpacking");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&crx.archive))?;
    for i in 0..z.len() {
        let mut f = z.by_index(i)?;
        // `enclosed_name` rejects absolute paths and `..` (zip-slip).
        let Some(rel) = f.enclosed_name() else { continue };
        let out = tmp.join(rel);
        if f.is_dir() { std::fs::create_dir_all(&out)?; continue; }
        if let Some(p) = out.parent() { std::fs::create_dir_all(p)?; }
        let mut w = std::fs::File::create(&out)?;
        std::io::copy(&mut f, &mut w)?;
    }
    let mpath = tmp.join("manifest.json");
    let raw = std::fs::read_to_string(&mpath)?;
    let mut manifest: serde_json::Value = serde_json::from_str(raw.trim_start_matches('\u{feff}'))?;
    manifest["key"] = B64.encode(&crx.public_key).into();
    std::fs::write(&mpath, serde_json::to_vec_pretty(&manifest)?)?;
    let _ = std::fs::remove_dir_all(dir);
    std::fs::rename(&tmp, dir)?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn id_derivation() {
        // Known vector: an empty key hashes to e3b0c442…, mapped to a–p.
        assert_eq!(id_from_key(b""), "odlameecjipmbmbejkplpemijjgpljce");
    }
    #[test]
    fn rejects_garbage() {
        assert!(parse(b"PK\x03\x04", None).is_err());
        let mut fake = b"Cr24".to_vec();
        fake.extend(2u32.to_le_bytes());
        fake.extend(0u32.to_le_bytes());
        fake.push(0);
        assert!(parse(&fake, None).unwrap_err().to_string().contains("version"));
    }
    /// Uses real store packages when available (EMERALD_TEST_CRX=dir with *.crx).
    #[test]
    fn real_crx_if_available() {
        let Ok(dir) = std::env::var("EMERALD_TEST_CRX") else { return };
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            if e.path().extension().is_some_and(|x| x == "crx") {
                let data = std::fs::read(e.path()).unwrap();
                let crx = parse(&data, None).unwrap_or_else(|err| panic!("{}: {err}", e.path().display()));
                assert_eq!(id_from_key(&crx.public_key), crx.id);
                // Tampering breaks the signature.
                let mut bad = data.clone();
                let n = bad.len() - 100;
                bad[n] ^= 0xff;
                assert!(parse(&bad, Some(&crx.id)).is_err());
                let out = std::env::temp_dir().join(format!("em-crx-{}", crx.id));
                let m = unpack(&crx, &out).unwrap();
                assert!(m["key"].is_string());
                let _ = std::fs::remove_dir_all(out);
                eprintln!("verified {} -> {}", e.path().display(), crx.id);
            }
        }
    }
}
