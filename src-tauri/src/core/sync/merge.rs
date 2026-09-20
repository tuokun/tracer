use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension};

use crate::core::{
    db,
    repo::{AppMetadataRecord, CategoryRecord, DeviceRecord, SegmentRecord},
};

use super::{Result, SyncError};

pub fn merge_segments(
    left: &[SegmentRecord],
    right: &[SegmentRecord],
) -> Result<Vec<SegmentRecord>> {
    let mut merged: BTreeMap<(String, i64), SegmentRecord> = BTreeMap::new();
    for item in left.iter().chain(right) {
        let key = (item.origin_device_id.clone(), item.segment_sequence);
        if let Some(old) = merged.get(&key) {
            if old != item {
                return Err(SyncError::Format(
                    "会话唯一键相同但内容不同，同步已停止".into(),
                ));
            }
        } else {
            merged.insert(key, item.clone());
        }
    }
    Ok(merged.into_values().collect())
}

pub fn merge_devices(left: &[DeviceRecord], right: &[DeviceRecord]) -> Result<Vec<DeviceRecord>> {
    let mut merged: BTreeMap<String, DeviceRecord> = left
        .iter()
        .map(|v| (v.device_id.clone(), v.clone()))
        .collect();
    for incoming in right {
        match merged.get(&incoming.device_id) {
            Some(old) if old.metadata_revision > incoming.metadata_revision => {}
            Some(old) if old.metadata_revision == incoming.metadata_revision && old != incoming => {
                return Err(SyncError::Format("设备元数据同版本内容冲突".into()))
            }
            _ => {
                merged.insert(incoming.device_id.clone(), incoming.clone());
            }
        }
    }
    Ok(merged.into_values().collect())
}

pub fn merge_apps(
    left: &[AppMetadataRecord],
    right: &[AppMetadataRecord],
) -> Result<Vec<AppMetadataRecord>> {
    let mut merged: BTreeMap<(String, i64), AppMetadataRecord> = left
        .iter()
        .map(|v| ((v.origin_device_id.clone(), v.origin_app_id), v.clone()))
        .collect();
    for incoming in right {
        let key = (incoming.origin_device_id.clone(), incoming.origin_app_id);
        match merged.get(&key) {
            Some(old) if old.metadata_revision > incoming.metadata_revision => {}
            Some(old) if old.metadata_revision == incoming.metadata_revision && old != incoming => {
                return Err(SyncError::Format("应用元数据同版本内容冲突".into()))
            }
            _ => {
                merged.insert(key, incoming.clone());
            }
        }
    }
    Ok(merged.into_values().collect())
}

fn category_order(v: &CategoryRecord) -> (i64, &str) {
    (v.logical_revision, &v.revision_device_id)
}
pub fn merge_categories(left: &[CategoryRecord], right: &[CategoryRecord]) -> Vec<CategoryRecord> {
    let mut merged: BTreeMap<String, CategoryRecord> = left
        .iter()
        .map(|v| (v.sync_id.clone(), v.clone()))
        .collect();
    for incoming in right {
        if merged
            .get(&incoming.sync_id)
            .is_none_or(|old| category_order(incoming) > category_order(old))
        {
            merged.insert(incoming.sync_id.clone(), incoming.clone());
        }
    }
    merged.into_values().collect()
}

/// 合并 manifest 元数据到本机。当前设备拥有的设备和应用元数据永远不接受远端覆盖。
pub fn apply_metadata(
    conn: &Connection,
    devices: &[DeviceRecord],
    apps: &[AppMetadataRecord],
    categories: &[CategoryRecord],
) -> Result<usize> {
    let local = db::local_device_id(conn)?;
    let tx = conn.unchecked_transaction()?;
    let mut changed = 0;
    for category in categories {
        let current: Option<(i64, String)> = tx
            .query_row(
                "SELECT logical_revision,revision_device_id FROM categories WHERE sync_id=?1",
                [&category.sync_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if current.as_ref().is_none_or(|v| {
            (
                category.logical_revision,
                category.revision_device_id.as_str(),
            ) > (v.0, v.1.as_str())
        }) {
            tx.execute("INSERT INTO categories(sync_id,name,color,rules,logical_revision,revision_device_id,is_deleted) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(sync_id) DO UPDATE SET name=excluded.name,color=excluded.color,rules=excluded.rules,logical_revision=excluded.logical_revision,revision_device_id=excluded.revision_device_id,is_deleted=excluded.is_deleted",params![category.sync_id,category.name,category.color,category.rules,category.logical_revision,category.revision_device_id,category.is_deleted])?;
            changed += 1;
        }
    }
    for device in devices {
        if device.device_id == local {
            continue;
        }
        let revision: Option<i64> = tx
            .query_row(
                "SELECT metadata_revision FROM devices WHERE device_id=?1",
                [&device.device_id],
                |r| r.get(0),
            )
            .optional()?;
        if revision.is_none_or(|r| device.metadata_revision > r) {
            tx.execute("INSERT INTO devices(device_id,display_name,metadata_revision) VALUES(?1,?2,?3) ON CONFLICT(device_id) DO UPDATE SET display_name=excluded.display_name,metadata_revision=excluded.metadata_revision",params![device.device_id,device.display_name,device.metadata_revision])?;
            changed += 1;
        }
    }
    for app in apps {
        if app.origin_device_id == local {
            continue;
        }
        let existing:Option<(i64,i64,String)>=tx.query_row("SELECT id,metadata_revision,process_name FROM apps WHERE origin_device_id=?1 AND origin_app_id=?2",params![app.origin_device_id,app.origin_app_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((_, _, process)) = &existing {
            if !process.eq_ignore_ascii_case(&app.process_name) {
                return Err(SyncError::Format("同一来源应用标识对应了不同进程名".into()));
            }
        }
        if existing
            .as_ref()
            .is_none_or(|v| app.metadata_revision > v.1)
        {
            let category_id: Option<i64> = match &app.category_sync_id {
                Some(id) => tx
                    .query_row(
                        "SELECT id FROM categories WHERE sync_id=?1 AND is_deleted=0",
                        [id],
                        |r| r.get(0),
                    )
                    .optional()?,
                None => None,
            };
            tx.execute("INSERT INTO apps(origin_device_id,origin_app_id,process_name,system_display_name,custom_alias,category_id,is_ignored,metadata_revision) VALUES(?1,?2,?3,?4,?5,COALESCE(?6,0),?7,?8) ON CONFLICT(origin_device_id,origin_app_id) DO UPDATE SET system_display_name=excluded.system_display_name,custom_alias=excluded.custom_alias,category_id=excluded.category_id,is_ignored=excluded.is_ignored,metadata_revision=excluded.metadata_revision",params![app.origin_device_id,app.origin_app_id,app.process_name,app.system_display_name,app.custom_alias,category_id,app.is_ignored,app.metadata_revision])?;
            changed += 1;
        }
    }
    tx.commit()?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn segment(device: &str, seq: i64, end: i64) -> SegmentRecord {
        SegmentRecord {
            origin_device_id: device.into(),
            segment_sequence: seq,
            origin_app_id: 1,
            process_name: "x.exe".into(),
            start_utc: 0,
            end_utc: end,
            source_local_date: 20260101,
            utc_offset_minutes: 0,
        }
    }
    #[test]
    fn segment_union_has_set_laws() {
        let a = vec![segment("a", 1, 1)];
        let b = vec![segment("b", 1, 2)];
        let c = vec![segment("c", 1, 3)];
        assert_eq!(
            merge_segments(&a, &b).unwrap(),
            merge_segments(&b, &a).unwrap()
        );
        assert_eq!(merge_segments(&a, &a).unwrap(), a);
        assert_eq!(
            merge_segments(&merge_segments(&a, &b).unwrap(), &c).unwrap(),
            merge_segments(&a, &merge_segments(&b, &c).unwrap()).unwrap()
        );
    }
    #[test]
    fn conflicting_segment_stops() {
        assert!(merge_segments(&[segment("a", 1, 1)], &[segment("a", 1, 2)]).is_err());
    }
    #[test]
    fn category_order_converges() {
        let a = CategoryRecord {
            sync_id: "x".into(),
            name: "a".into(),
            color: None,
            rules: None,
            logical_revision: 2,
            revision_device_id: "a".into(),
            is_deleted: false,
        };
        let mut b = a.clone();
        b.name = "b".into();
        b.revision_device_id = "b".into();
        assert_eq!(
            merge_categories(&[a.clone()], &[b.clone()]),
            merge_categories(&[b.clone()], &[a.clone()])
        );
        assert_eq!(merge_categories(&[a], &[b])[0].name, "b");
    }
}
