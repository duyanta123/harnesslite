//! Recognise the line `dsh web` prints when its server starts listening.
//!
//! The harness announces itself on stdout as `dsh web: http://127.0.0.1:52175`,
//! optionally with a token query — `dsh web: http://127.0.0.1:52175/?token=…`.
//! That string decides where the application points its WebView, so it is
//! validated rather than trusted: a supervised child must not be able to steer
//! the shell at an arbitrary origin by printing one.

use hd_core::contract::READY_LINE_PREFIX;

/// Outcome of inspecting one line of harness output.
#[derive(Debug, PartialEq, Eq)]
pub enum Ready {
    /// The announcement named a usable loopback address: the bare origin the
    /// bridge trusts, and the full URL the WebView loads — equal when the
    /// announcement carries no path or query.
    At { origin: String, url: String },
    /// The line announced something this shell refuses to load.
    Rejected(String),
}

/// Extract the address announced by one line of harness stdout.
///
/// Returns `None` for ordinary log output, which is most lines.
pub fn parse(line: &str) -> Option<Ready> {
    let announced = line.trim_end().strip_prefix(READY_LINE_PREFIX)?;
    // The announcement is a bare URL; anything after whitespace is commentary.
    let candidate = announced.split_whitespace().next().unwrap_or_default();

    let Ok(url) = url::Url::parse(candidate) else {
        return Some(Ready::Rejected(format!(
            "harness announced an unparseable URL: {candidate}"
        )));
    };

    let is_loopback = matches!(url.host_str(), Some("127.0.0.1") | Some("localhost"));
    if url.scheme() != "http" || !is_loopback {
        return Some(Ready::Rejected(format!(
            "harness announced a non-loopback URL: {candidate}"
        )));
    }
    if url.port().is_none() {
        return Some(Ready::Rejected(format!(
            "harness announced a URL without an explicit port: {candidate}"
        )));
    }

    Some(Ready::At {
        origin: url.origin().ascii_serialization(),
        url: candidate.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{parse, Ready};

    #[test]
    fn accepts_the_announcement_dsh_actually_prints() {
        assert_eq!(
            parse("dsh web: http://127.0.0.1:52175"),
            Some(Ready::At {
                origin: "http://127.0.0.1:52175".into(),
                url: "http://127.0.0.1:52175".into(),
            })
        );
    }

    /// Upstream gates the web app behind a token that rides the announcement
    /// as a query parameter. The bridge keeps trusting the bare origin; the
    /// WebView loads the announced URL whole.
    #[test]
    fn keeps_the_token_query_but_not_in_the_origin() {
        assert_eq!(
            parse(
                "dsh web: http://127.0.0.1:47661/?token=F1VUxAP8kfRLqwXv9wbB02tvrw5-6jBXp91GybhaojA"
            ),
            Some(Ready::At {
                origin: "http://127.0.0.1:47661".into(),
                url: "http://127.0.0.1:47661/?token=F1VUxAP8kfRLqwXv9wbB02tvrw5-6jBXp91GybhaojA".into(),
            })
        );
    }

    #[test]
    fn tolerates_trailing_whitespace_and_carriage_returns() {
        assert_eq!(
            parse("dsh web: http://localhost:3080/\r\n"),
            Some(Ready::At {
                origin: "http://localhost:3080".into(),
                url: "http://localhost:3080/".into(),
            })
        );
    }

    #[test]
    fn ignores_ordinary_log_output() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("loading plugin dsh-tool-bash"), None);
        assert_eq!(parse("  dsh web: http://127.0.0.1:1"), None);
    }

    #[test]
    fn refuses_to_be_steered_off_the_loopback() {
        assert!(matches!(
            parse("dsh web: http://example.com:80"),
            Some(Ready::Rejected(_))
        ));
        assert!(matches!(
            parse("dsh web: https://127.0.0.1:443"),
            Some(Ready::Rejected(_))
        ));
        assert!(matches!(
            parse("dsh web: file:///etc/passwd"),
            Some(Ready::Rejected(_))
        ));
    }

    #[test]
    fn requires_an_explicit_port() {
        assert!(matches!(
            parse("dsh web: http://127.0.0.1"),
            Some(Ready::Rejected(_))
        ));
    }

    #[test]
    fn rejects_garbage_after_the_marker() {
        assert!(matches!(
            parse("dsh web: not-a-url"),
            Some(Ready::Rejected(_))
        ));
    }
}
