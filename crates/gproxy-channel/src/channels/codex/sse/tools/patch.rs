use crate::channel::ChannelError;
use gproxy_protocol::wire::openai::responses::{
    ApplyPatchCreate, ApplyPatchDelete, ApplyPatchOperation, ApplyPatchUpdate,
};

#[derive(Clone, Copy)]
enum PatchKind {
    Create,
    Delete,
    Update,
}

pub(super) fn patch_operation(input: &str) -> Result<ApplyPatchOperation, ChannelError> {
    let mut lines = input.lines();
    lines
        .find(|line| *line == "*** Begin Patch")
        .ok_or_else(|| {
            ChannelError::InvalidResponse("apply_patch input missing begin marker".into())
        })?;
    let header = lines.next().ok_or_else(|| {
        ChannelError::InvalidResponse("apply_patch input missing operation".into())
    })?;
    let (kind, path) = if let Some(path) = header.strip_prefix("*** Add File: ") {
        (PatchKind::Create, path)
    } else if let Some(path) = header.strip_prefix("*** Delete File: ") {
        (PatchKind::Delete, path)
    } else if let Some(path) = header.strip_prefix("*** Update File: ") {
        (PatchKind::Update, path)
    } else {
        return Err(ChannelError::InvalidResponse(
            "apply_patch input has an unknown operation".into(),
        ));
    };
    if path.trim().is_empty() {
        return Err(ChannelError::InvalidResponse(
            "apply_patch input has an empty path".into(),
        ));
    }
    let mut ended = false;
    let mut diff = Vec::new();
    for line in lines {
        if line == "*** End Patch" {
            ended = true;
            break;
        }
        if line.starts_with("*** Add File: ")
            || line.starts_with("*** Delete File: ")
            || line.starts_with("*** Update File: ")
            || line.starts_with("*** Move to: ")
        {
            return Err(ChannelError::InvalidResponse("apply_patch has multiple operations or a move that cannot fit one native operation".into()));
        }
        diff.push(if matches!(kind, PatchKind::Create) {
            line.strip_prefix('+').unwrap_or(line)
        } else {
            line
        });
    }
    if !ended {
        return Err(ChannelError::InvalidResponse(
            "apply_patch input missing end marker".into(),
        ));
    }
    let diff = diff.join("\n");
    let diff = if diff.is_empty() {
        diff
    } else {
        format!("{diff}\n")
    };
    Ok(match kind {
        PatchKind::Create => ApplyPatchOperation::Create(ApplyPatchCreate {
            diff,
            path: path.into(),
            rest: Default::default(),
        }),
        PatchKind::Delete => ApplyPatchOperation::Delete(ApplyPatchDelete {
            path: path.into(),
            rest: Default::default(),
        }),
        PatchKind::Update => ApplyPatchOperation::Update(ApplyPatchUpdate {
            diff,
            path: path.into(),
            rest: Default::default(),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_patch_never_becomes_an_empty_update() {
        for input in [
            "not a patch",
            "*** Begin Patch\n*** Update File: \n*** End Patch",
            "*** Begin Patch\n*** Future File: x\n*** End Patch",
            "*** Begin Patch\n*** Update File: x",
        ] {
            assert!(patch_operation(input).is_err(), "{input}");
        }
    }
}
