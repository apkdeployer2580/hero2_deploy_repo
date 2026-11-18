pub const H2_FLIGHT_TOWNS: [(u8, u8, u8); 6] = [(0, 7, 22), (34, 9, 6), (64, 7, 12), (67, 15, 10), (72, 15, 7), (76, 6, 16)];
pub const H2_FLIGHT_GM_FIRST: u32 = 92;
pub const H2_FLIGHT_GM_EMPTY: u32 = 98;
const H2_FLIGHT_FLAGS_SLOT: u32 = 0x0019_3a4c;
const H2_RETURN_SLOT: u32 = 0x0019_3bdc;

pub fn h2_is_flight(p: &H2BagPopup) -> bool {
    p.record.kind == 28 && p.record.id == 1
}

fn h2_flight_flags(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<(u32, u8)> {
    let (base, len) = h2_array(read, h2_static_handle(read, H2_FLIGHT_FLAGS_SLOT)?)?;
    (len == 32).then_some(())?;
    Some((base + 31, h2_rd_u8(read, base + 31)?))
}

pub fn h2_flight_visit_write(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<(u32, Vec<u8>)> {
    let state = h2_rd_u32(read, 0x0019_3aec)?;
    matches!(state, 3 | 4 | 5 | 8 | 12).then_some(())?;
    let map = h2_stage_map_no(read, &GuestBuild::Hero2Ktf.hero2_save()?.area)?;
    let home = h2_static_handle(read, H2_RETURN_SLOT).and_then(|h| h2_byte_array(read, h)).filter(|a| a.len() == 3);
    let (addr, old) = h2_flight_flags(read)?;
    let mut next = old;
    for (i, &(town, _, _)) in H2_FLIGHT_TOWNS.iter().enumerate() {
        if map == town || home.as_ref().is_some_and(|a| a[0] == town) {
            next |= 1 << i;
        }
    }
    (next != old).then_some((addr, vec![next]))
}

pub fn h2_flight_destinations(read: &impl Fn(u32, usize) -> Vec<u8>) -> Vec<usize> {
    let Some((_, flags)) = h2_flight_flags(read) else { return Vec::new() };
    (0..H2_FLIGHT_TOWNS.len()).filter(|i| flags & (1 << i) != 0).collect()
}

pub fn h2_flight_picker(read: &impl Fn(u32, usize) -> Vec<u8>, h2: &Hero2Java, p: &H2BagPopup) -> Option<Vec<usize>> {
    h2_is_flight(p).then_some(())?;
    p.labels.iter().map(|label| (0..H2_FLIGHT_TOWNS.len()).find(|i| h2_gm_row_handle(read, h2, H2_FLIGHT_GM_FIRST + *i as u32) == Some(*label))).collect()
}

pub fn h2_flight_picker_writes(read: &impl Fn(u32, usize) -> Vec<u8>, h2: &Hero2Java, p: &H2BagPopup) -> Option<Vec<(u32, Vec<u8>)>> {
    h2_is_flight(p).then_some(())?;
    let towns = h2_flight_destinations(read);
    (!towns.is_empty() && towns.len() as u32 <= h2_object_array_capacity(p.labels_len)).then_some(())?;
    let mut writes = Vec::new();
    for (i, town) in towns.iter().enumerate() {
        let label = h2_gm_row_handle(read, h2, H2_FLIGHT_GM_FIRST + *town as u32)?;
        writes.push((p.labels_fields + 8 + i as u32 * 4, label.to_le_bytes().to_vec()));
    }
    let len = towns.len() as u32;
    writes.push((p.labels_fields + 4, len.to_le_bytes().to_vec()));
    writes.push((p.fields + 4 + Hero2Java::WIN_CURSOR_OFF, vec![0]));
    writes.extend(h2_move_row_attach_writes(p, p.labels_handle, len));
    Some(writes)
}

pub fn h2_flight_allowed(read: &impl Fn(u32, usize) -> Vec<u8>) -> bool {
    GuestBuild::Hero2Ktf.hero2_save().and_then(|sv| h2_stage_map_no(read, &sv.area)).is_some_and(|map| map < 101)
        && h2_rd_u32(read, 0x0019_3aec) == Some(5)
}
