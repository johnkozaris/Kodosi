use zeroize::Zeroizing;

use super::*;
use crate::{
    identity::{
        device_cert::DeviceCertificate,
        keys::DeviceKeys,
        signed_device_list::{DeviceListEntry, SignedDeviceList},
    },
    network::BackendConfig,
};

fn devices(user: &str, device: &str, key: &[u8]) -> VerifiedIdentity {
    let certificate = DeviceCertificate {
        user_id: user.to_owned(),
        device_id: device.to_owned(),
        device_label: "Laptop".into(),
        signer_device_id: device.to_owned(),
        sig_public_key: key.to_vec(),
        issued_at_ms: 0,
        expires_at_ms: None,
    };
    VerifiedIdentity {
        generation: 1,
        devices: BTreeMap::from([(device.to_owned(), certificate)]),
        list: SignedDeviceList {
            user_id: user.to_owned(),
            generation: 1,
            entries: vec![DeviceListEntry {
                device_id: device.to_owned(),
                signer_device_id: device.to_owned(),
            }],
            signer_device_id: device.to_owned(),
            issued_at_ms: 0,
            expires_at_ms: None,
        },
        root: [0; 32],
    }
}

#[tokio::test]
async fn only_a_signed_device_of_this_person_can_call_this_computer() {
    let root = tempfile::tempdir().unwrap();
    let network = BackendClient::new(BackendConfig {
        api_url: reqwest::Url::parse("http://127.0.0.1:1/").unwrap(),
        issuer: "http://127.0.0.1:1".into(),
        client_id: "test".into(),
        scopes: vec![],
        data_root: root.path().to_owned(),
        secret_service: "test".into(),
        isolated: true,
    })
    .unwrap();
    let user = Uuid::now_v7().to_string();
    let credentials = Credentials {
        user_id: user.clone(),
        display_name: "Ana".into(),
        token: Zeroizing::new("token".into()),
        keys: Arc::new(DeviceKeys::generate().unwrap()),
        enrolled: true,
        notice: None,
        generation: network.generation(),
        cancel: CancellationToken::new(),
    };
    let laptop = Uuid::now_v7().to_string();
    let key = [1; 32];
    network.keep_identity(
        &credentials,
        &user,
        &devices(&user, &laptop, &key),
        "tag".into(),
    );

    let friend = Uuid::now_v7().to_string();
    let refused = admit(&network, &credentials, (&friend, &laptop), &key)
        .await
        .unwrap_err();
    assert_eq!(
        refused.message,
        "Only your own devices can start terminals on this computer."
    );
    let recovery = "00000000-0000-8000-8000-000000000000";
    assert_eq!(
        admit(&network, &credentials, (&user, recovery), &key)
            .await
            .unwrap_err()
            .code,
        "access"
    );
    assert!(
        admit(&network, &credentials, (&user, &laptop), &[2; 32])
            .await
            .is_err()
    );
    let unknown = Uuid::now_v7().to_string();
    assert!(
        admit(&network, &credentials, (&user, &unknown), &key)
            .await
            .is_err()
    );
    assert_eq!(
        admit(&network, &credentials, (&user, &laptop), &key).await,
        Ok(())
    );
}
