//! Which background work (ETW, timers) must run, given what is visible. Pure.

use crate::model::{Avail, Availability, DriverState};
use crate::settings::Metrics;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Effects {
    pub etw: bool,
    pub sample_timer: bool,
    pub graph_timer: bool,
    pub detect_timer: bool,
}

pub fn plan_effects(visible: bool, config_open: bool, candidate: bool, m: &Metrics) -> Effects {
    let watching = visible || config_open;
    Effects {
        etw: candidate || (watching && (m.fps || m.graph)),
        sample_timer: watching,
        graph_timer: visible && m.graph,
        detect_timer: candidate,
    }
}

pub fn effective_metrics(m: &Metrics, a: &Availability, driver: DriverState) -> Metrics {
    let ok = |av: &Avail| matches!(av, Avail::Ok);
    Metrics {
        fps: m.fps && ok(&a.fps),
        graph: m.graph && ok(&a.fps),
        gpu_use: m.gpu_use && ok(&a.gpu_use),
        gpu_temp: m.gpu_temp && ok(&a.gpu_temp),
        cpu_use: m.cpu_use && ok(&a.cpu_use),
        cpu_temp: m.cpu_temp && ok(&a.cpu_temp) && driver == DriverState::Installed,
    }
}

#[cfg(test)]
mod tests {
use super::*;
use crate::model::{Avail, Availability, DriverState};
use crate::settings::Metrics;
fn all() -> Metrics { Metrics { cpu_temp: true, ..Metrics::default() } }
#[test]
fn idle_runs_nothing() {
    assert_eq!(plan_effects(false, false, false, &all()), Effects::default());
}
#[test]
fn effects_by_situation() {
    assert_eq!(plan_effects(true, false, false, &all()), Effects { etw: true, sample_timer: true, graph_timer: true, detect_timer: false });
    let no_frames = Metrics { fps: false, graph: false, ..all() };
    assert_eq!(plan_effects(true, false, false, &no_frames), Effects { etw: false, sample_timer: true, graph_timer: false, detect_timer: false });
    assert_eq!(plan_effects(false, false, true, &all()), Effects { etw: true, sample_timer: false, graph_timer: false, detect_timer: true });
    assert_eq!(plan_effects(false, true, false, &all()), Effects { etw: true, sample_timer: true, graph_timer: false, detect_timer: false });
}
#[test]
fn effective_metrics_drop_unavailable_and_driverless() {
    let mut a = Availability::all_ok();
    a.gpu_temp = Avail::NoSensor { reason: "x".into() };
    let e = effective_metrics(&all(), &a, DriverState::Missing);
    assert!(!e.gpu_temp && !e.cpu_temp && e.fps && e.gpu_use);
    assert!(effective_metrics(&all(), &Availability::all_ok(), DriverState::Installed).cpu_temp);
    a.fps = Avail::NoSensor { reason: "x".into() };
    assert!(!effective_metrics(&all(), &a, DriverState::Installed).graph);
}
}
