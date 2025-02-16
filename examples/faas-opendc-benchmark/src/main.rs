use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;

use dslab_core::simulation::Simulation;
use dslab_faas::coldstart::FixedTimeColdStartPolicy;
use dslab_faas::config::Config;
use dslab_faas::function::{Application, Function};
use dslab_faas::resource::{ResourceConsumer, ResourceProvider};
use dslab_faas::simulation::ServerlessSimulation;
use dslab_faas_extra::opendc_trace::process_opendc_trace;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let trace = process_opendc_trace(Path::new(&args[1]));

    let mut config: Config = Default::default();
    config.disable_contention = true;
    config.coldstart_policy = Rc::new(RefCell::new(FixedTimeColdStartPolicy::new(120.0 * 60.0, 0.0)));
    let mut sim = ServerlessSimulation::new(Simulation::new(1), config);
    for _ in 0..18 {
        let mem = sim.create_resource("mem", 4096);
        sim.add_host(None, ResourceProvider::new(vec![mem]), 4);
    }
    for app in trace.iter() {
        let mut max_mem = 0;
        for sample in app.iter() {
            max_mem = usize::max(max_mem, sample.mem_provisioned);
        }
        let mem = sim.create_resource_requirement("mem", max_mem as u64);
        let app_id = sim.add_app(Application::new(1, 0.5, 1., ResourceConsumer::new(vec![mem])));
        let fn_id = sim.add_function(Function::new(app_id));
        for sample in app.iter() {
            if sample.invocations == 0 {
                continue;
            }
            for _ in 0..sample.invocations {
                sim.send_invocation_request(fn_id, (sample.exec as f64) / 1000.0, (sample.time as f64) / 1000.0);
            }
        }
    }
    let t = Instant::now();
    sim.step_until_no_events();
    let elapsed = t.elapsed().as_secs_f64();
    let stats = sim.get_stats();
    println!(
        "processed {} invocations and {} events in {:.2} seconds ({:.2} events per sec)",
        stats.invocations,
        sim.event_count(),
        elapsed,
        (sim.event_count() as f64) / elapsed
    );
}
