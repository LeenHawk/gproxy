strict_string_enum!(ImageBackground {
    Transparent => "transparent",
    Opaque => "opaque",
    Auto => "auto",
});

strict_string_enum!(ImageResponseBackground {
    Transparent => "transparent",
    Opaque => "opaque",
});

strict_string_enum!(ImageModeration {
    Low => "low",
    Auto => "auto",
});

strict_string_enum!(ImageOutputFormat {
    Png => "png",
    Jpeg => "jpeg",
    Webp => "webp",
});

strict_string_enum!(ImageQuality {
    Standard => "standard",
    Hd => "hd",
    Low => "low",
    Medium => "medium",
    High => "high",
    Auto => "auto",
});

strict_string_enum!(ImageEditQuality {
    Low => "low",
    Medium => "medium",
    High => "high",
    Auto => "auto",
});

strict_string_enum!(ImageResponseQuality {
    Low => "low",
    Medium => "medium",
    High => "high",
});

strict_string_enum!(ImageResponseFormat {
    Url => "url",
    B64Json => "b64_json",
});

extensible_string_enum!(ImageSize, ImageSizeKnown {
    Auto => "auto",
    Size1024By1024 => "1024x1024",
    Size1536By1024 => "1536x1024",
    Size1024By1536 => "1024x1536",
    Size256By256 => "256x256",
    Size512By512 => "512x512",
    Size1792By1024 => "1792x1024",
    Size1024By1792 => "1024x1792",
});

strict_string_enum!(ImageEditSize {
    Auto => "auto",
    Size1024By1024 => "1024x1024",
    Size1536By1024 => "1536x1024",
    Size1024By1536 => "1024x1536",
});

extensible_string_enum!(ResponseImageGenerationSize, ResponseImageGenerationSizeKnown {
    Auto => "auto",
    Size1024By1024 => "1024x1024",
    Size1024By1536 => "1024x1536",
    Size1536By1024 => "1536x1024",
});

strict_string_enum!(ImageResponseSize {
    Size1024By1024 => "1024x1024",
    Size1024By1536 => "1024x1536",
    Size1536By1024 => "1536x1024",
});

strict_string_enum!(ImageStyle {
    Vivid => "vivid",
    Natural => "natural",
});

strict_string_enum!(ImageInputFidelity {
    High => "high",
    Low => "low",
});

extensible_string_enum!(ImageStreamEventType, ImageStreamEventTypeKnown {
    ImageGenerationPartialImage => "image_generation.partial_image",
    ImageGenerationCompleted => "image_generation.completed",
    ImageEditPartialImage => "image_edit.partial_image",
    ImageEditCompleted => "image_edit.completed",
});
