use std::path::Path;

use log::*;

pub mod sample;
pub mod sfz;
pub mod voice;

fn main() {
    env_logger::builder().filter_level(LevelFilter::Info).init();

    let instrument = std::thread::spawn(|| {
        // let sfz_path = Path::new("/home/wmedrano/Downloads/SteinwayModelB/steinway-b.sfz");
        let sfz_path = Path::new("/home/wmedrano/Downloads/VSUpright1_SFZ/Upright No 1.sfz");
        info!("Creating instrument for {:?}.", sfz_path);
        let instrument = sfz::Instrument::from_path(Path::new(sfz_path));
        info!("Loaded sfz for {:?}.", sfz_path);
        instrument
    });

    let (client, status) =
        jack::Client::new("lyre-sample", jack::ClientOptions::NO_START_SERVER).unwrap();
    info!(
        "Initialized JACK client {} with status {:?}.",
        client.name(),
        status
    );

    let midi = client.register_port("midi", jack::MidiIn).unwrap();
    let mut outputs = [
        client.register_port("out_l", jack::AudioOut).unwrap(),
        client.register_port("out_r", jack::AudioOut).unwrap(),
    ];
    let mut instrument = instrument.join().unwrap();
    let async_client = client
        .activate_async(
            (),
            jack::ClosureProcessHandler::new(move |_, ps| {
                match &mut outputs {
                    [a, b] => instrument.play(
                        midi.iter(ps).map(|m| (m.time as usize, m.bytes)),
                        a.as_mut_slice(ps),
                        b.as_mut_slice(ps),
                    ),
                };
                jack::Control::Continue
            }),
        )
        .unwrap();
    std::thread::park();
    drop(async_client);
}
