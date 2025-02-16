use std::boxed::Box;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use dslab_core::simulation::Simulation;
use dslab_faas::coldstart::FixedTimeColdStartPolicy;
use dslab_faas::config::Config;
use dslab_faas::function::{Application, Function};
use dslab_faas::resource::{ResourceConsumer, ResourceProvider};
use dslab_faas::scheduler::Scheduler;
use dslab_faas::simulation::ServerlessSimulation;
use dslab_faas::stats::Stats;
use dslab_faas_extra::azure_trace::{process_azure_trace, Trace};
use dslab_faas_extra::hermes::HermesScheduler;
use dslab_faas_extra::simple_schedulers::*;

fn test_scheduler(scheduler: Box<dyn Scheduler>, trace: &Trace, time_range: f64) -> Stats {
    let mut config: Config = Default::default();
    config.scheduler = scheduler;
    config.coldstart_policy = Rc::new(RefCell::new(FixedTimeColdStartPolicy::new(20.0 * 60.0, 0.0)));
    let mut sim = ServerlessSimulation::new(Simulation::new(1), config);
    for _ in 0..10 {
        let mem = sim.create_resource("mem", 4096 * 4);
        sim.add_host(None, ResourceProvider::new(vec![mem]), 4);
    }
    for app in trace.app_records.iter() {
        let mem = sim.create_resource_requirement("mem", app.mem);
        sim.add_app(Application::new(
            1,
            app.cold_start,
            1.,
            ResourceConsumer::new(vec![mem]),
        ));
    }
    for func in trace.function_records.iter() {
        sim.add_function(Function::new(func.app_id));
    }
    for req in trace.trace_records.iter() {
        sim.send_invocation_request(req.id as u64, req.dur, req.time);
    }
    sim.set_simulation_end(time_range);
    sim.step_until_no_events();
    sim.get_stats()
}

fn print_results(stats: Stats, name: &str) {
    println!("describing {}", name);
    println!("- {} successful invocations", stats.invocations);
    println!(
        "- cold start rate = {}",
        (stats.cold_starts as f64) / (stats.invocations as f64)
    );
    println!(
        "- wasted memory time = {}",
        stats.wasted_resource_time.get(&0).unwrap().sum()
    );
    println!(
        "- mean absolute execution slowdown = {}",
        stats.abs_exec_slowdown.mean()
    );
    println!(
        "- mean relative execution slowdown = {}",
        stats.rel_exec_slowdown.mean()
    );
    println!("- mean absolute total slowdown = {}", stats.abs_total_slowdown.mean());
    println!("- mean relative total slowdown = {}", stats.rel_total_slowdown.mean());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let trace = process_azure_trace(Path::new(&args[1]), 200000);
    println!(
        "trace processed successfully, {} invocations",
        trace.trace_records.len()
    );
    let mut time_range = 0.0;
    for req in trace.trace_records.iter() {
        time_range = f64::max(time_range, req.time + req.dur);
    }
    print_results(
        test_scheduler(
            Box::new(LocalityBasedScheduler::new(None, None, true)),
            &trace,
            time_range,
        ),
        "Locality-based, warm only",
    );
    print_results(
        test_scheduler(
            Box::new(LocalityBasedScheduler::new(None, None, false)),
            &trace,
            time_range,
        ),
        "Locality-based, allow cold",
    );
    print_results(
        test_scheduler(Box::new(RandomScheduler::new(1)), &trace, time_range),
        "Random",
    );
    print_results(
        test_scheduler(Box::new(LeastLoadedScheduler::new(true)), &trace, time_range),
        "Least-loaded",
    );
    print_results(
        test_scheduler(Box::new(RoundRobinScheduler::new()), &trace, time_range),
        "Round Robin",
    );
    print_results(
        test_scheduler(Box::new(HermesScheduler::new()), &trace, time_range),
        "Hermes",
    );
}
