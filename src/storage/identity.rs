use crate::model::Project;
use std::collections::HashSet;

pub(super) fn validate(project: &Project) -> Result<(), String> {
    let mut ids = HashSet::new();
    let mut note_ids = HashSet::new();
    let mut check = |value: &str, kind: &str| {
        let id =
            uuid::Uuid::parse_str(value).map_err(|_| format!("工程校验失败：{kind} UUID 无效"))?;
        if !ids.insert(id) {
            return Err(format!("工程校验失败：{kind} UUID 重复"));
        }
        Ok(())
    };
    check(&project.uuid, "工程")?;
    for group in project
        .library
        .iter()
        .chain(project.tracks.iter().map(|track| &track.main_group))
    {
        check(&group.uuid, "音符组")?;
        for note in &group.notes {
            if note.uuid.len() != 16 || !note.uuid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("工程校验失败：音符 UUID 必须为 16 位十六进制标识".into());
            }
            let id =
                u64::from_str_radix(&note.uuid, 16).map_err(|_| "工程校验失败：音符 UUID 无效")?;
            if !note_ids.insert(id) {
                return Err("工程校验失败：音符 UUID 重复".into());
            }
        }
    }
    for track in &project.tracks {
        for reference in track.references() {
            check(&reference.uuid, "音符组实例")?;
        }
    }
    Ok(())
}
