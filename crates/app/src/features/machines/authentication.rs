//! Decide whether selecting a machine needs its stored SSH secret.
use machines::remote_connection::{AuthMethod, RemoteConnection};

pub(super) fn needs_stored_secret(remote: &RemoteConnection) -> bool {
    match remote.auth.as_ref() {
        None | Some(AuthMethod::None) => false,
        // A terminal opens an independent connection, so a connected metadata
        // session cannot replace a password missing from this snapshot.
        Some(AuthMethod::Password(password)) => password.is_empty(),
        // A successful key connection already establishes that this runtime
        // snapshot can authenticate. Cold snapshots may need a saved passphrase.
        Some(AuthMethod::AuthKey { passphrase, .. }) => {
            passphrase.as_ref().is_none_or(String::is_empty) && !remote.connected()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn connection(auth: Option<AuthMethod>) -> RemoteConnection {
        let mut remote = RemoteConnection::default();
        remote.user = "user".into();
        remote.host = "host".into();
        remote.auth = auth;
        remote
    }

    fn key(passphrase: Option<&str>) -> RemoteConnection {
        connection(Some(AuthMethod::AuthKey {
            pubkey: None,
            privatekey: PathBuf::from("id_rsa"),
            passphrase: passphrase.map(str::to_owned),
        }))
    }

    #[test]
    fn agent_and_unspecified_authentication_do_not_request_stored_secrets() {
        for auth in [None, Some(AuthMethod::None)] {
            let remote = connection(auth);
            for connected in [false, true] {
                remote.set_connected(connected);
                assert!(!needs_stored_secret(&remote));
            }
        }
    }

    #[test]
    fn only_cold_keys_without_cached_passphrases_request_stored_secrets() {
        for (passphrase, cold_needs_secret) in [
            (None, true),
            (Some(""), true),
            (Some("secret"), false),
            (Some(" \t "), false),
        ] {
            let remote = key(passphrase);
            for (connected, expected) in [(false, cold_needs_secret), (true, false)] {
                remote.set_connected(connected);
                assert_eq!(needs_stored_secret(&remote), expected);
            }
        }
    }

    #[test]
    fn empty_passwords_need_lookup_even_when_the_metadata_session_is_connected() {
        for (password, expected) in [("", true), ("secret", false), (" \t ", false)] {
            let remote = connection(Some(AuthMethod::Password(password.into())));
            for connected in [false, true] {
                remote.set_connected(connected);
                assert_eq!(needs_stored_secret(&remote), expected);
            }
        }
    }

    #[test]
    fn reloaded_protected_key_requires_a_stored_passphrase() -> Result<(), serde_json::Error> {
        let remote = key(Some("private secret"));
        remote.set_connected(true);
        assert!(!needs_stored_secret(&remote));
        let reloaded: RemoteConnection = serde_json::from_str(&serde_json::to_string(&remote)?)?;
        assert!(!reloaded.connected());
        assert!(matches!(
            reloaded.auth.as_ref(),
            Some(AuthMethod::AuthKey {
                passphrase: None,
                ..
            })
        ));
        assert!(needs_stored_secret(&reloaded));
        Ok(())
    }
}
