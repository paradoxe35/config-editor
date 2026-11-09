//! What the host writes once and reads in several places (a YAML alias or
//! merge) written out where the edit parts it from the rest, and kept
//! where only a key holding it is renamed (a Compose service renamed keeps
//! its `<<: *env` and `logging: *log`).

use super::super::codec;
use super::super::language::ConfigLanguage;
use serde_json::Value;

/// `previous` with the shared values `target` parts written out, reading
/// as `old`; `None` when nothing is parted. A rename moves no shared
/// value: compared with the renames undone, unless renaming the keys
/// where they are would not read as `target` (a key renamed inside an
/// anchor its aliases read too).
pub(super) fn detached(
    host: &dyn ConfigLanguage,
    previous: &str,
    (old, target): (&Value, &Value),
    shape: Option<&Value>,
) -> Option<String> {
    let reads_old = |text: &String| codec::read(host, text, shape).ok().as_ref() == Some(old);
    let undone = codec::with_renames_undone(old, target);
    if undone != *target {
        let kept = host.detach_shared(previous, old, &undone).filter(reads_old);
        let base = kept.as_deref().unwrap_or(previous);
        if codec::patch_renames(host, base, old, target, shape).is_some() {
            return kept;
        }
    }
    host.detach_shared(previous, old, target).filter(reads_old)
}
