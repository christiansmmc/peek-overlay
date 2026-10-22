//! 3D engine usage per adapter, summed over every process, like Task Manager's "3D" graph.

use super::{Luid, PdhQuery, luid_from_parts, pdh_value_ok, percent};
use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_MORE_DATA, PdhGetFormattedCounterArrayW,
};

const COUNTER: &str = r"\GPU Engine(*)\Utilization Percentage";

/// One `GPU Engine` counter instance, e.g. `pid_1234_luid_0x00000000_0x0000D1B7_phys_0_eng_0_engtype_3D`.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineInstance {
    pub pid: u32,
    pub luid: Luid,
    pub engtype: String,
}

pub fn parse_engine_instance(name: &str) -> Option<EngineInstance> {
    let (pid, rest) = name.strip_prefix("pid_")?.split_once("_luid_")?;
    let (luid, rest) = rest.split_once("_phys_")?;
    let (high, low) = luid.split_once('_')?;
    let (_, engtype) = rest.split_once("_engtype_")?;
    Some(EngineInstance {
        pid: pid.parse().ok()?,
        luid: luid_from_parts(parse_hex(high)? as i32, parse_hex(low)?),
        engtype: engtype.into(),
    })
}

fn parse_hex(s: &str) -> Option<u32> {
    u32::from_str_radix(s.strip_prefix("0x")?, 16).ok()
}

/// Sums the `3D` engines of all processes per adapter, limited to 100, and picks the adapter:
/// the game's (its busiest 3D instance), else `default_luid`, else the busiest adapter.
/// An adapter without 3D instances (e.g. an idle, powered-down dedicated GPU) reads 0.
pub fn pick_adapter_usage(
    samples: &[(EngineInstance, f64)],
    game_pid: Option<u32>,
    default_luid: Option<Luid>,
) -> Option<(Luid, u8)> {
    let three_d = || samples.iter().filter(|(inst, _)| inst.engtype == "3D");
    let mut sums: Vec<(Luid, f64)> = Vec::new();
    for (inst, value) in three_d() {
        match sums.iter_mut().find(|(luid, _)| *luid == inst.luid) {
            Some((_, sum)) => *sum += value,
            None => sums.push((inst.luid, *value)),
        }
    }
    let game = game_pid.and_then(|pid| {
        three_d()
            .filter(|(inst, _)| inst.pid == pid)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(inst, _)| inst.luid)
    });
    let busiest = || sums.iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|(luid, _)| *luid);
    let luid = game.or(default_luid).or_else(busiest)?;
    let sum = sums.iter().find(|(l, _)| *l == luid).map_or(0.0, |(_, sum)| *sum);
    Some((luid, percent(sum)))
}

pub struct GpuUsage {
    query: PdhQuery,
    /// Reused across samples; `u64` keeps the PDH item array 8-byte aligned.
    buf: Vec<u64>,
}

impl GpuUsage {
    pub fn new() -> Result<Self, String> {
        Ok(Self { query: PdhQuery::open(COUNTER)?, buf: Vec::new() })
    }

    pub fn sample(&mut self, game_pid: Option<u32>, default_luid: Option<Luid>) -> Option<(Luid, u8)> {
        if !self.query.collect() {
            return None;
        }
        let samples = self.read_instances()?;
        pick_adapter_usage(&samples, game_pid, default_luid)
    }

    /// Every instance of the last collect that has a valid value.
    fn read_instances(&mut self) -> Option<Vec<(EngineInstance, f64)>> {
        let counter = self.query.counter();
        let count = fill_array(&mut self.buf, |size, count, items| unsafe {
            PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, size, count, items)
        })?;
        // SAFETY: PDH wrote `count` items at the start of `buf`, and their names after them.
        let items = unsafe { std::slice::from_raw_parts(self.buf.as_ptr().cast::<Item>(), count) };
        let samples = items
            .iter()
            .filter(|item| pdh_value_ok(item.FmtValue.CStatus))
            .filter_map(|item| {
                let inst = parse_engine_instance(&unsafe { item.szName.to_string() }.ok()?)?;
                Some((inst, unsafe { item.FmtValue.Anonymous.doubleValue }))
            })
            .collect();
        Some(samples)
    }
}

type Item = PDH_FMT_COUNTERVALUE_ITEM_W;

/// How many times a fill that hit PDH_MORE_DATA is retried with the updated size.
const FILL_ATTEMPTS: u32 = 3;

/// PDH's two-call protocol for `PdhGetFormattedCounterArrayW`: ask for the size, then fill `buf`.
/// An instance can appear between the calls (e.g. a game starting), so a fill that still gets
/// PDH_MORE_DATA is retried with the size PDH just reported. Returns the item count.
fn fill_array(
    buf: &mut Vec<u64>,
    mut call: impl FnMut(&mut u32, &mut u32, Option<*mut Item>) -> u32,
) -> Option<usize> {
    let (mut size, mut count) = (0u32, 0u32);
    let mut status = call(&mut size, &mut count, None);
    for _ in 0..FILL_ATTEMPTS {
        if status != PDH_MORE_DATA {
            break;
        }
        buf.resize((size as usize).div_ceil(size_of::<u64>()), 0);
        status = call(&mut size, &mut count, Some(buf.as_mut_ptr().cast()));
    }
    let fits = size as usize <= buf.len() * size_of::<u64>();
    (status == 0 && fits).then_some(count as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inst(pid: u32, luid: Luid, e: &str) -> EngineInstance { EngineInstance { pid, luid, engtype: e.into() } }
    #[test]
    fn parses_pdh_engine_instance_names() {
        let e = parse_engine_instance("pid_1234_luid_0x00000000_0x0000D1B7_phys_0_eng_0_engtype_3D").unwrap();
        assert_eq!(e, inst(1234, 0xD1B7, "3D"));
        let e = parse_engine_instance("pid_88_luid_0x00000001_0x0000ABCD_phys_0_eng_3_engtype_VideoDecode").unwrap();
        assert_eq!(e, inst(88, 0x1_0000_ABCD, "VideoDecode"));
        assert!(parse_engine_instance("_Total").is_none());
    }
    #[test]
    fn picks_game_adapter_and_sums_3d_of_all_processes() {
        let (a, b) = (0xA, 0xB);
        let s = vec![(inst(100, a, "3D"), 60.0), (inst(200, a, "3D"), 30.0), (inst(100, a, "Copy"), 50.0), (inst(300, b, "3D"), 10.0)];
        assert_eq!(pick_adapter_usage(&s, Some(100), Some(b)), Some((a, 90)));
        assert_eq!(pick_adapter_usage(&s, None, Some(b)), Some((b, 10)));
        assert_eq!(pick_adapter_usage(&s, Some(999), None), Some((a, 90)));
        assert_eq!(pick_adapter_usage(&[(inst(1, a, "3D"), 80.0), (inst(2, a, "3D"), 45.0)], None, None), Some((a, 100)));
    }
    #[test]
    fn idle_default_adapter_without_3d_instances_reads_zero() {
        // Laptop: dedicated GPU powered down (no instances), integrated GPU busy.
        let (igpu, dgpu) = (0xA, 0xD);
        let s = vec![(inst(100, igpu, "3D"), 40.0), (inst(100, igpu, "Copy"), 5.0)];
        assert_eq!(pick_adapter_usage(&s, None, Some(dgpu)), Some((dgpu, 0)));
        assert_eq!(pick_adapter_usage(&[], None, Some(dgpu)), Some((dgpu, 0)));
    }
    #[test]
    fn array_fill_retries_more_data_with_the_updated_size() {
        // An instance appears between the size query and the fill: the first fill is too small.
        let mut buf = Vec::new();
        let mut need = 48;
        let n = fill_array(&mut buf, |size, count, items| {
            if items.is_none() || *size < need {
                *size = need;
                need = 64;
                return PDH_MORE_DATA;
            }
            *count = 2;
            0
        });
        assert_eq!(n, Some(2));
        assert!(buf.len() * size_of::<u64>() >= 64);
        // Instances keep appearing: give up after a few attempts instead of looping forever.
        let mut calls = 0;
        let n = fill_array(&mut buf, |size, _, _| {
            calls += 1;
            *size = 1024 * calls;
            PDH_MORE_DATA
        });
        assert_eq!(n, None);
        assert_eq!(calls, 1 + FILL_ATTEMPTS);
    }
}
