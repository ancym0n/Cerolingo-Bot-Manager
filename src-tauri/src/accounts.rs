//! `accounts.txt`: one `email:password:cookie-header` per line.

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
};

use base64::Engine;
use serde_json::{Value, json};

use crate::paths;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub email: String,
    pub password: String,
    pub jwt_token: String,
    pub cookies: String,
}

/// First non-empty `jwt_token=<value>` in a cookie header (value ends at `;` or whitespace).
fn find_jwt(cookies: &str) -> Option<&str> {
    let mut rest = cookies;
    while let Some(pos) = rest.find("jwt_token=") {
        let tail = &rest[pos + "jwt_token=".len()..];
        let end = tail.find(|c: char| c == ';' || c.is_whitespace()).unwrap_or(tail.len());
        if end > 0 {
            return Some(&tail[..end]);
        }
        rest = tail;
    }
    None
}

pub fn parse_line(line: &str) -> Option<Account> {
    let line = line.trim();
    let (email, rest) = line.split_once(':')?;
    let (password, cookies) = rest.split_once(':')?;
    if email.is_empty() || password.is_empty() {
        return None;
    }
    let jwt = find_jwt(cookies)?;
    Some(Account {
        email: email.to_owned(),
        password: password.to_owned(),
        jwt_token: jwt.to_owned(),
        cookies: cookies.to_owned(),
    })
}

/// Accounts that have a usable `jwt_token` (others are skipped).
pub fn load() -> Vec<Account> {
    fs::read_to_string(paths::accounts_path())
        .map(|text| text.lines().filter_map(parse_line).collect())
        .unwrap_or_default()
}

/// Every saved `name=value` pair as a CDP cookie on `.duolingo.com`.
pub fn browser_cookies(account: &Account) -> Vec<Value> {
    let mut map = BTreeMap::new();
    for part in account.cookies.split(';') {
        if let Some((name, value)) = part.trim().split_once('=')
            && !name.is_empty()
            && !value.is_empty()
        {
            map.insert(name, value);
        }
    }
    map.insert("jwt_token", account.jwt_token.as_str());
    map.into_iter()
        .map(|(name, value)| {
            json!({
                "name": name, "value": value, "domain": ".duolingo.com", "path": "/",
                "secure": true, "sameSite": "Lax",
            })
        })
        .collect()
}

/// Duolingo user id stored in the JWT `sub` claim.
pub fn user_id(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    match serde_json::from_slice::<Value>(&bytes).ok()?.get("sub")? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub fn ensure_file() -> std::io::Result<std::path::PathBuf> {
    let path = paths::accounts_path();
    fs::create_dir_all(paths::logs_dir())?;
    fs::OpenOptions::new().create(true).append(true).open(&path)?;
    Ok(path)
}

pub fn append(email: &str, password: &str, cookies: &str) -> std::io::Result<()> {
    let path = ensure_file()?;
    let mut file = fs::OpenOptions::new().append(true).open(path)?;
    file.write_all(format!("{email}:{password}:{cookies}\n").as_bytes())
}

/// Replace the cookie part of an existing `email:password:` line.
pub fn update(email: &str, password: &str, cookies: &str) -> std::io::Result<()> {
    let path = paths::accounts_path();
    let text = fs::read_to_string(&path)?;
    let prefix = format!("{email}:{password}:");
    let mut out = String::with_capacity(text.len() + cookies.len());
    for line in text.lines() {
        if line.starts_with(&prefix) {
            out.push_str(&prefix);
            out.push_str(cookies);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    paths::write_atomic(&path, out.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines_with_token() {
        let acc = parse_line(" a@b.c:pw:foo=1; jwt_token=abc.def.ghi; bar=2 ").unwrap();
        assert_eq!(acc.email, "a@b.c");
        assert_eq!(acc.password, "pw");
        assert_eq!(acc.jwt_token, "abc.def.ghi");
        assert!(parse_line("a@b.c:pw:foo=1").is_none());
        assert!(parse_line(":pw:jwt_token=x").is_none());
        assert_eq!(parse_line("a:b:jwt_token=; jwt_token=y").unwrap().jwt_token, "y");
    }

    #[test]
    fn reads_user_id_from_jwt() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"sub":12345}"#);
        assert_eq!(user_id(&format!("h.{payload}.s")).as_deref(), Some("12345"));
        assert_eq!(user_id("garbage"), None);
    }

    #[test]
    fn cookies_include_token_once() {
        let acc = parse_line("e:p:jwt_token=old; a=1; =x; b=").unwrap();
        let cookies = browser_cookies(&acc);
        assert_eq!(cookies.len(), 2);
        assert!(cookies.iter().any(|c| c["name"] == "jwt_token" && c["value"] == "old"));
    }
}
