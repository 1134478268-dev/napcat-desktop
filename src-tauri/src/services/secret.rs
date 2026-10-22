//! 本地敏感字段加密（审计 M3）。
//!
//! 为什么用 DPAPI 而不是自己搓 AES：
//!   · DPAPI 的密钥由 Windows 绑定到「当前用户 + 当前机器」，不需要我们自己保管主密钥，
//!     也就没有"主密钥放哪"这个死结（放配置文件 = 白加密，写死在代码里 = 等于没有）；
//!   · 同用户的其他进程仍能解密，所以它防的是"配置文件被拷走/被同步到网盘/被截图"，
//!     不防同机同用户的恶意程序 —— 这一点在文档里如实写清楚，不夸大。
//!
//! 存储格式：`dpapi:<hex>`。没有前缀的一律当作旧版明文（向后兼容），
//! 下次保存时会自动升级成密文。

/// 加密前缀。改动它会导致旧密文无法识别，不要动。
pub const PREFIX: &str = "dpapi:";

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("hex 长度必须是偶数".to_string());
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let hi = (bytes[i] as char).to_digit(16).ok_or("hex 解码失败")?;
        let lo = (bytes[i + 1] as char).to_digit(16).ok_or("hex 解码失败")?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

#[cfg(windows)]
fn dpapi(protect: bool, data: &[u8]) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut out = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    // SAFETY: 两个 blob 都指向本次调用期间有效的内存；out 由系统分配，成功后用 LocalFree 释放。
    let ok = unsafe {
        if protect {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        }
    };

    if ok == 0 {
        return Err(if protect {
            "DPAPI 加密失败".to_string()
        } else {
            "DPAPI 解密失败（配置可能由其他用户/机器加密）".to_string()
        });
    }

    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    unsafe {
        LocalFree(out.pbData as *mut core::ffi::c_void);
    }
    Ok(bytes)
}

/// 加密一个敏感字符串。空串原样返回（不产生无意义的密文）。
pub fn encrypt(plain: &str) -> Result<String, String> {
    if plain.is_empty() {
        return Ok(String::new());
    }
    #[cfg(windows)]
    {
        let cipher = dpapi(true, plain.as_bytes())?;
        Ok(format!("{PREFIX}{}", hex_encode(&cipher)))
    }
    #[cfg(not(windows))]
    {
        // 非 Windows 没有 DPAPI：明确报错而不是静默落明文。
        Err("当前平台不支持 DPAPI 加密".to_string())
    }
}

/// 解密。没有 `dpapi:` 前缀视为旧版明文，原样返回。
pub fn decrypt(stored: &str) -> Result<String, String> {
    let Some(hex) = stored.strip_prefix(PREFIX) else {
        return Ok(stored.to_string());
    };
    #[cfg(windows)]
    {
        let cipher = hex_decode(hex)?;
        let plain = dpapi(false, &cipher)?;
        String::from_utf8(plain).map_err(|e| format!("解密结果不是合法 UTF-8: {e}"))
    }
    #[cfg(not(windows))]
    {
        let _ = hex;
        Err("当前平台不支持 DPAPI 解密".to_string())
    }
}

/// 是否已是密文形态。
pub fn is_encrypted(stored: &str) -> bool {
    stored.starts_with(PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_passthrough_for_legacy_config() {
        // 旧配置是明文，必须能读，不能强制迁移
        assert_eq!(decrypt("sk-legacy-plain").unwrap(), "sk-legacy-plain");
        assert!(!is_encrypted("sk-legacy-plain"));
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(encrypt("").unwrap(), "");
    }

    #[test]
    fn roundtrip_on_windows() {
        let secret = "sk-test-abcdef123456";
        let cipher = encrypt(secret).unwrap();
        if cipher.is_empty() {
            return; // 非 Windows 平台会返回 Err，这里不会走到
        }
        assert!(is_encrypted(&cipher));
        assert_ne!(cipher, secret, "密文不能等于明文");
        assert_eq!(decrypt(&cipher).unwrap(), secret);
    }
}
