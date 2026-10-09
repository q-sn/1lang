//! API keys live in the OS credential store (Windows Credential Manager),
//! never in settings.json.

const SERVICE: &str = "1lang";

fn entry(provider_id: &str) -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, &format!("provider:{provider_id}"))
}

pub fn get(provider_id: &str) -> Option<String> {
    entry(provider_id).ok()?.get_password().ok().filter(|k| !k.is_empty())
}

pub fn set(provider_id: &str, key: Option<&str>) -> Result<(), String> {
    let e = entry(provider_id).map_err(|e| e.to_string())?;
    match key.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => e.set_password(k).map_err(|e| e.to_string()),
        None => match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(err.to_string()),
        },
    }
}

/// "••••abcd" for display.
pub fn hint(provider_id: &str) -> Option<String> {
    let k = get(provider_id)?;
    let tail: String = k.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    Some(format!("••••{tail}"))
}
