use gproxy_channel_api::{ClientProfile, ClientProfilePreset};

pub(super) static CLIENT_PROFILE: ClientProfile =
    ClientProfile::preset(ClientProfilePreset::Chrome148);
