use anyhow::{Result, anyhow};
use mail_parser::{HeaderValue, MessageParser};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug)]
#[allow(dead_code)]
pub struct PatchsetMetadata {
    pub message_id: String,
    pub subject: String,
    pub author: String,
    pub date: i64,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub index: u32,
    pub total: u32,
    pub to: String,
    pub cc: String,
    pub is_patch_or_cover: bool,
    pub version: Option<u32>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Patch {
    pub message_id: String,
    pub body: String,
    pub diff: String,
    pub part_index: u32,
}

pub fn parse_email(raw_email: &[u8]) -> Result<(PatchsetMetadata, Option<Patch>)> {
    let message = MessageParser::default()
        .parse(raw_email)
        .ok_or_else(|| anyhow!("Failed to parse email"))?;

    let message_id = message
        .message_id()
        .ok_or_else(|| anyhow!("No Message-ID header"))?
        .to_string();

    let subject = message.subject().unwrap_or("(no subject)").to_string();

    let author = message
        .from()
        .and_then(|addr| addr.first())
        .map(|a| {
            let name = a.name().unwrap_or_default();
            let address = a.address().unwrap_or("unknown");
            if name.is_empty() {
                address.to_string()
            } else {
                format!("{} <{}>", name, address)
            }
        })
        .unwrap_or_else(|| "unknown".to_string());

    let date = message.date().map(|d| d.to_timestamp()).unwrap_or(0);

    let to = message
        .to()
        .map(|addr| {
            addr.iter()
                .map(|a| a.address().unwrap_or("").to_string())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();

    let cc = message
        .cc()
        .map(|addr| {
            addr.iter()
                .map(|a| a.address().unwrap_or("").to_string())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();

    let in_reply_to = match message.in_reply_to() {
        HeaderValue::Text(t) => Some(t.to_string()),
        HeaderValue::TextList(l) => l.first().map(|s| s.to_string()),
        _ => None,
    };

    let references = match message.references() {
        HeaderValue::Text(t) => vec![t.to_string()],
        HeaderValue::TextList(l) => l.iter().map(|s| s.to_string()).collect(),
        _ => vec![],
    };

    let (index, total) = parse_subject_index(&subject);
    let version = parse_subject_version(&subject);

    let body = message.body_text(0).unwrap_or_default().to_string();

    let diff = if body.contains("diff --git") {
        body.clone()
    } else {
        String::new()
    };

    // Detection logic
    let subject_lower = subject.to_lowercase();
    let is_reply = subject_lower.trim().starts_with("re:");
    let has_patch_tag = subject_lower.contains("patch") || subject_lower.contains("rfc");
    let has_diff = !diff.is_empty();

    let has_series_marker = total > 1 || (total == 1 && index == 1 && subject.contains("1/1"));

    // It is a patch or cover letter if:
    // 1. It is NOT a reply (Re: ...)
    // 2. AND (It has [PATCH]/[RFC] tag OR contains a diff OR looks like a series part)
    let is_patch_or_cover = !is_reply && (has_patch_tag || has_diff || has_series_marker);

    let metadata = PatchsetMetadata {
        message_id: message_id.clone(),
        subject,
        author,
        date,
        in_reply_to,
        references,
        index,
        total,
        to,
        cc,
        is_patch_or_cover,
        version,
    };

    let patch = if has_diff {
        Some(Patch {
            message_id,
            body,
            diff,
            part_index: index,
        })
    } else {
        None
    };

    Ok((metadata, patch))
}

fn parse_subject_index(subject: &str) -> (u32, u32) {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Allow [Anything 1/2 Anything]
    let re = RE.get_or_init(|| Regex::new(r"\[.*?(\d+)/(\d+).*?\]").unwrap());

    if let Some(caps) = re.captures(subject) {
        let index = caps.get(1).map_or(1, |m| m.as_str().parse().unwrap_or(1));
        let total = caps.get(2).map_or(1, |m| m.as_str().parse().unwrap_or(1));
        (index, total)
    } else {
        (1, 1)
    }
}

pub fn parse_subject_version(subject: &str) -> Option<u32> {
    static RE_VER: OnceLock<Regex> = OnceLock::new();
    // Match [PATCH v2 ...] or [PATCH ... v2]
    // We look for " v(\d+) " or " v(\d+)]" or " v(\d+)/"
    // Actually, simple \[.*?v(\d+).*?\] might match "dev" in "device".
    // We want word boundary or specific format.
    // Usually it is " v2 " or "-v2" or space before v.
    // Let's try flexible but safe: ` v(\d+)[^a-z]`
    let re = RE_VER.get_or_init(|| Regex::new(r"(?:^|[ \[(\/-])v(\d+)(?:[ \])\/)]|$)").unwrap());

    if let Some(caps) = re.captures(subject) {
        caps.get(1).and_then(|m| m.as_str().parse().ok())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_author_parsing() {
        let raw =
            b"Message-ID: <123>\r\nFrom: Test User <test@example.com>\r\nSubject: Test\r\n\r\nBody";
        let (meta, _) = parse_email(raw).unwrap();
        assert_eq!(meta.author, "Test User <test@example.com>");

        let raw_no_name =
            b"Message-ID: <456>\r\nFrom: test2@example.com\r\nSubject: Test\r\n\r\nBody";
        let (meta2, _) = parse_email(raw_no_name).unwrap();
        assert_eq!(meta2.author, "test2@example.com");
    }

    #[test]
    fn test_reply_with_diff_is_not_patchset() {
        // A message that starts with Re: but contains diff --git
        // This simulates a reply quoting a patch or sending an inline fixup
        let raw = b"Message-ID: <123>\r\nSubject: Re: [PATCH] fix bug\r\n\r\n> diff --git a/file b/file\n> index...";
        let (meta, _) = parse_email(raw).unwrap();
        
        // This fails with current logic because has_diff is true
        assert!(!meta.is_patch_or_cover, "Reply with diff should NOT be a patchset");
    }

    #[test]
    fn test_normal_patch() {
        let raw = b"Message-ID: <456>\r\nSubject: [PATCH] fix bug\r\n\r\ndiff --git a/file b/file\nindex...";
        let (meta, _) = parse_email(raw).unwrap();
        assert!(meta.is_patch_or_cover);
    }
    
    #[test]
    fn test_cover_letter() {
        let raw = b"Message-ID: <789>\r\nSubject: [PATCH 0/5] fix bug\r\n\r\nCover letter body";
        let (meta, _) = parse_email(raw).unwrap();
        assert!(meta.is_patch_or_cover);
    }

    #[test]
    fn test_pure_reply() {
        let raw = b"Message-ID: <abc>\r\nSubject: Re: [PATCH] fix bug\r\n\r\nLGTM";
        let (meta, _) = parse_email(raw).unwrap();
        assert!(!meta.is_patch_or_cover);
    }

    #[test]
    fn test_rfc_patch_parsing() {
        let subject = "[RFC PATCH 1/3] My RFC";
        let (index, total) = parse_subject_index(subject);
        assert_eq!(index, 1);
        assert_eq!(total, 3);
    }

    #[test]
    fn test_complex_prefix_parsing() {
        let subject = "[PATCH v2 net-next 02/14] Something";
        let (index, total) = parse_subject_index(subject);
        assert_eq!(index, 2);
        assert_eq!(total, 14);
    }

    #[test]
    fn test_no_patch_prefix_parsing() {
        // Some lists might just use [RFC 1/2]
        let subject = "[RFC 1/2] Just RFC";
        let (index, total) = parse_subject_index(subject);
        assert_eq!(index, 1);
        assert_eq!(total, 2);
    }
}
