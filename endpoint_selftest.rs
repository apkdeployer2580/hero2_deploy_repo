// Standalone self-test of the private-server endpoint parser.
//
//   rustc --edition 2021 --test endpoint_selftest.rs -o t3.exe && ./t3.exe
//
// ★ This does NOT mirror the implementation — it `include!`s the real
// `wie_android/src/endpoint.rs`, so the parser under test IS the shipping parser
// and the two cannot drift. (`cargo test -p wie_android` crashes at startup on
// this Windows host because of the linked winit/android deps; see
// `items_selftest.rs` for the same workaround, which does have to mirror because
// its logic lives next to android types. `landscape_selftest.rs` was the other such
// mirror until 2026-09-18, when it was deleted for having drifted into a lie.)
//
// Why this parser is worth testing at all: it decides where the game connects.
// A wrong answer either bricks the online features for a tester or, worse, sends
// their login id somewhere unintended — and the input is a hand-typed / pasted
// string from a player, which is the least trustworthy input in the app.

include!("wie_android/src/endpoint.rs");

#[cfg(test)]
mod tests {
    use super::*;

    /// (host, port, tls) — flattened so the assertions below stay one-liners.
    fn p(s: &str) -> Option<(String, u16, bool)> {
        parse_endpoint(s).map(|e| (e.host, e.port, e.tls))
    }

    #[test]
    fn bare_host_gets_the_default_port() {
        assert_eq!(p("hero3.example.com"), Some(("hero3.example.com".into(), 8091, false)));
        assert_eq!(DEFAULT_SERVER_PORT, 8091);
    }

    #[test]
    fn explicit_port_is_used() {
        assert_eq!(p("hero3.example.com:12345"), Some(("hero3.example.com".into(), 12345, false)));
        // A tunnel handing out a high random port is the reason the port is
        // configurable at all (it used to be a hardcoded 8091 constant).
        assert_eq!(p("1.tcp.ngrok.io:54321"), Some(("1.tcp.ngrok.io".into(), 54321, false)));
    }

    #[test]
    fn ipv4_literals_work() {
        assert_eq!(p("127.0.0.1"), Some(("127.0.0.1".into(), 8091, false)));
        assert_eq!(p("192.168.0.5:8091"), Some(("192.168.0.5".into(), 8091, false)));
    }

    #[test]
    fn whitespace_and_trailing_newline_are_stripped() {
        assert_eq!(p("  hero3.example.com:8091  \n"), Some(("hero3.example.com".into(), 8091, false)));
        assert_eq!(p("hero3.example.com\r\n"), Some(("hero3.example.com".into(), 8091, false)));
    }

    #[test]
    fn pasted_url_forms_are_tolerated() {
        // Players paste what they were given; a scheme or trailing slash should
        // not be the difference between working and silently not working.
        assert_eq!(p("tcp://hero3.example.com:8091"), Some(("hero3.example.com".into(), 8091, false)));
        assert_eq!(p("http://hero3.example.com/"), Some(("hero3.example.com".into(), 8091, false)));
    }

    #[test]
    fn only_the_first_line_is_read() {
        assert_eq!(p("hero3.example.com\nsecond.example.com"), Some(("hero3.example.com".into(), 8091, false)));
    }

    #[test]
    fn empty_and_commented_files_fall_through() {
        // None = "use the next config layer", NOT "connect to nothing".
        assert_eq!(p(""), None);
        assert_eq!(p("   \n"), None);
        assert_eq!(p("# set your server here"), None);
    }

    #[test]
    fn garbage_is_rejected_rather_than_guessed() {
        assert_eq!(p(":8091"), None, "empty host");
        assert_eq!(p("host:"), None, "empty port");
        assert_eq!(p("host:0"), None, "port 0 is not connectable");
        assert_eq!(p("host:99999"), None, "port out of u16 range");
        assert_eq!(p("host:80x1"), None, "non-numeric port");
        assert_eq!(p("hero3 example.com"), None, "space inside a hostname");
        assert_eq!(p("hero3.example.com/path"), None, "a path is not a hostname");
        assert_eq!(p("hero3.example.com;rm"), None, "punctuation is not a hostname");
        assert_eq!(p(&format!("{}:8091", "a".repeat(254))), None, "over the DNS name limit");
    }

    #[test]
    fn ipv6_literals_are_not_mangled_into_host_port() {
        // The bug this pins: splitting on the last colon turned "::1" into host
        // ":" port 1 — a connect to something the player never typed. An
        // unbracketed multi-colon address is the WHOLE host.
        assert_eq!(p("::1"), Some(("::1".into(), 8091, false)));
        assert_eq!(p("fe80::1"), Some(("fe80::1".into(), 8091, false)));
        // Ambiguous by construction: "fe80::1:8091" is a valid address on its own,
        // so it must NOT be read as address + port. Brackets are how you say port.
        assert_eq!(p("fe80::1:8091"), Some(("fe80::1:8091".into(), 8091, false)));
    }

    #[test]
    fn bracketed_form_gives_an_ipv6_address_a_port() {
        assert_eq!(p("[::1]:9000"), Some(("::1".into(), 9000, false)));
        assert_eq!(p("[fe80::1]"), Some(("fe80::1".into(), 8091, false)));
        assert_eq!(p("[::1]:0"), None, "port 0 still rejected");
        assert_eq!(p("[::1"), None, "unclosed bracket");
        assert_eq!(p("[::1]junk"), None, "trailing junk after the bracket");
    }

    #[test]
    fn hostname_punctuation_that_is_legal_is_kept() {
        assert_eq!(p("my-server.example.co.kr"), Some(("my-server.example.co.kr".into(), 8091, false)));
        assert_eq!(p("my_host:8091"), Some(("my_host".into(), 8091, false)));
    }

    // --- transport selection (2026-07-31) -----------------------------------
    // The account id is a bearer credential, so a PUBLIC address must be tls://
    // while the local adb-reverse loop stays plain. Both come from the same build,
    // so the scheme is the switch.

    #[test]
    fn tls_scheme_selects_encryption() {
        assert_eq!(p("tls://hero3.example.com"), Some(("hero3.example.com".into(), 8091, true)));
        assert_eq!(p("tls://hero3.example.com:9443"), Some(("hero3.example.com".into(), 9443, true)));
        assert_eq!(p("TLS://hero3.example.com"), Some(("hero3.example.com".into(), 8091, true)), "case-insensitive");
        assert_eq!(p("  tls://hero3.example.com  \n"), Some(("hero3.example.com".into(), 8091, true)));
        assert_eq!(p("tls://[::1]:9443"), Some(("::1".into(), 9443, true)));
    }

    #[test]
    fn absent_or_other_scheme_stays_plain() {
        // Plain is the default so the existing dev loop and any already-written
        // config file keep working untouched.
        assert_eq!(p("hero3.example.com").map(|e| e.2), Some(false));
        assert_eq!(p("tcp://hero3.example.com").map(|e| e.2), Some(false));
        assert_eq!(p("http://hero3.example.com").map(|e| e.2), Some(false));
        // Not "tls" — must not be mistaken for it.
        assert_eq!(p("tlsx://hero3.example.com").map(|e| e.2), Some(false));
    }

    #[test]
    fn pin_accepts_the_shapes_fingerprint_tools_print() {
        let want = [0xabu8; 32];
        let plain: String = (0..32).map(|_| "ab").collect();
        assert_eq!(parse_pin(&plain), Some(want));
        assert_eq!(parse_pin(&format!("sha256:{plain}")), Some(want));
        assert_eq!(parse_pin(&format!("  {plain}  \n")), Some(want));
        // openssl prints colon-separated pairs.
        let colons: String = (0..32).map(|_| "AB").collect::<Vec<_>>().join(":");
        assert_eq!(parse_pin(&colons), Some(want), "colon-separated + uppercase");
    }

    #[test]
    fn pin_rejects_anything_not_exactly_32_bytes() {
        assert_eq!(parse_pin(""), None);
        assert_eq!(parse_pin("abcd"), None, "too short");
        assert_eq!(parse_pin(&"ab".repeat(33)), None, "too long");
        assert_eq!(parse_pin(&format!("{}zz", "ab".repeat(31))), None, "non-hex");
    }
}
