use serde::{Deserialize, Serialize};

macro_rules! extensible_string {
    ($name:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(untagged)]
        pub enum $name {
            Known($known),
            Unknown(String),
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $known {
            $(#[serde(rename = $wire)] $variant),+
        }
    };
}

extensible_string!(VideoModelId, KnownVideoModelId {
    Sora2 => "sora-2",
    Sora2Pro => "sora-2-pro",
});
extensible_string!(VideoSeconds, KnownVideoSeconds {
    Four => "4", Eight => "8", Twelve => "12",
});
extensible_string!(VideoExtensionSeconds, KnownVideoExtensionSeconds {
    Four => "4", Eight => "8", Twelve => "12", Sixteen => "16", Twenty => "20",
});
extensible_string!(VideoSize, KnownVideoSize {
    Portrait720x1280 => "720x1280",
    Landscape1280x720 => "1280x720",
    Portrait1024x1792 => "1024x1792",
    Landscape1792x1024 => "1792x1024",
});
extensible_string!(VideoContentVariant, KnownVideoContentVariant {
    Video => "video", Thumbnail => "thumbnail", Spritesheet => "spritesheet",
});
extensible_string!(VideoListOrder, KnownVideoListOrder {
    Asc => "asc", Desc => "desc",
});
extensible_string!(VideoStatus, KnownVideoStatus {
    Queued => "queued", InProgress => "in_progress", Completed => "completed", Failed => "failed",
});
extensible_string!(VideoObjectType, KnownVideoObjectType {
    Video => "video",
});
extensible_string!(VideoDeletedObjectType, KnownVideoDeletedObjectType {
    VideoDeleted => "video.deleted",
});
extensible_string!(VideoListObjectType, KnownVideoListObjectType {
    List => "list",
});
