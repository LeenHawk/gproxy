macro_rules! video_string_enum {
    ($outer:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
        #[serde(untagged)]
        #[non_exhaustive]
        pub enum $outer {
            Known($known),
            Unknown(String),
        }

        impl<'de> serde::Deserialize<'de> for $outer {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                crate::protocol::extensible::deserialize_extensible(d, Self::Known, Self::Unknown)
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[non_exhaustive]
        pub enum $known {
            $(#[serde(rename = $wire)] $variant,)+
        }
    };
}

video_string_enum!(VideoModelId, VideoModelIdKnown {
    Sora2 => "sora-2",
    Sora2Pro => "sora-2-pro",
    Sora220251006 => "sora-2-2025-10-06",
    Sora2Pro20251006 => "sora-2-pro-2025-10-06",
    Sora220251208 => "sora-2-2025-12-08",
});

video_string_enum!(VideoSeconds, VideoSecondsKnown {
    Four => "4",
    Eight => "8",
    Twelve => "12",
});

video_string_enum!(VideoExtensionSeconds, VideoExtensionSecondsKnown {
    Four => "4",
    Eight => "8",
    Twelve => "12",
    Sixteen => "16",
    Twenty => "20",
});

video_string_enum!(VideoSize, VideoSizeKnown {
    Size720By1280 => "720x1280",
    Size1280By720 => "1280x720",
    Size1024By1792 => "1024x1792",
    Size1792By1024 => "1792x1024",
});

video_string_enum!(VideoContentVariant, VideoContentVariantKnown {
    Video => "video",
    Thumbnail => "thumbnail",
    Spritesheet => "spritesheet",
});

video_string_enum!(VideoListOrder, VideoListOrderKnown {
    Ascending => "asc",
    Descending => "desc",
});

video_string_enum!(VideoStatus, VideoStatusKnown {
    Queued => "queued",
    InProgress => "in_progress",
    Completed => "completed",
    Failed => "failed",
});

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum VideoObjectType {
    #[serde(rename = "video")]
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum VideoDeletedObjectType {
    #[serde(rename = "video.deleted")]
    VideoDeleted,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum VideoListObjectType {
    #[serde(rename = "list")]
    List,
}
