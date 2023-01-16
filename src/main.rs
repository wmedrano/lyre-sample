use log::*;

fn main() {
    env_logger::builder().filter_level(LevelFilter::Info).init();
    let (client, status) =
        jack::Client::new("lyre-sample", jack::ClientOptions::NO_START_SERVER).unwrap();
    info!(
        "Initialized JACK client {} with status {:?}.",
        client.name(),
        status
    );
    let mut outputs = [
        client.register_port("out_l", jack::AudioOut).unwrap(),
        client.register_port("out_r", jack::AudioOut).unwrap(),
    ];
    let async_client = client
        .activate_async(
            (),
            jack::ClosureProcessHandler::new(move |_, ps| {
                let mut output = match &mut outputs {
                    [a, b] => [a.as_mut_slice(ps), b.as_mut_slice(ps)],
                };
                for output_channel in output.iter_mut() {
                    clear(output_channel);
                }
                jack::Control::Continue
            }),
        )
        .unwrap();
    std::thread::park();
    drop(async_client);
}

fn clear(buffer: &mut [f32]) {
    for sample in buffer.iter_mut() {
        *sample = 0.0;
    }
}
