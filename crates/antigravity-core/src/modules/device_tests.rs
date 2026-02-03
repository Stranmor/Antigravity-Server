#[cfg(test)]
mod tests {
    use super::super::{generate_profile, new_standard_machine_id, random_hex, sync_to_state_db};
    use rusqlite::Connection;

    #[test]
    fn test_generate_profile() {
        let profile = generate_profile();

        assert!(profile.machine_id.starts_with("auth0|user_"));
        assert_eq!(profile.machine_id.len(), 11 + 32);

        assert_eq!(profile.mac_machine_id.len(), 36);
        assert!(profile.mac_machine_id.chars().nth(14) == Some('4'));

        assert_eq!(profile.dev_device_id.len(), 36);

        assert!(profile.sqm_id.starts_with('{'));
        assert!(profile.sqm_id.ends_with('}'));
        assert_eq!(profile.sqm_id.len(), 38);
    }

    #[test]
    fn test_random_hex() {
        let hex = random_hex(32);
        assert_eq!(hex.len(), 32);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(hex.chars().all(|c| c.is_lowercase() || c.is_numeric()));
    }

    #[test]
    fn test_new_standard_machine_id() {
        let id = new_standard_machine_id();
        assert_eq!(id.len(), 36);

        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);

        assert_eq!(parts[2].chars().next(), Some('4'));

        let variant = parts[3].chars().next().unwrap();
        assert!(
            variant == '8' || variant == '9' || variant == 'a' || variant == 'b',
            "variant should be 8-b, got {}",
            variant
        );
    }

    #[test]
    fn test_profiles_are_unique() {
        let p1 = generate_profile();
        let p2 = generate_profile();

        assert_ne!(p1.machine_id, p2.machine_id);
        assert_ne!(p1.mac_machine_id, p2.mac_machine_id);
        assert_ne!(p1.dev_device_id, p2.dev_device_id);
        assert_ne!(p1.sqm_id, p2.sqm_id);
    }

    #[test]
    fn test_sync_to_state_db() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("state.vscdb");

        Connection::open(&db_path).unwrap();

        let service_id = "test-service-id-12345";
        sync_to_state_db(&db_path, service_id).unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let result: String = conn
            .query_row(
                "SELECT value FROM ItemTable WHERE key = 'storage.serviceMachineId'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(result, service_id);
    }
}
