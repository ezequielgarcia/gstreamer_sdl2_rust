use std::env;
use std::sync::mpsc::channel;
use std::path::Path;
use std::process;
use std::time::Duration;
use url::Url;
use ctrlc;
use gstreamer::prelude::*;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Missing filename");
        process::exit(-1);
    }

    let input = &args[1];
    let source =
        if let Ok(url) = Url::parse(input) {
            let host = url.host_str().unwrap();
            if host.contains("youtu") {
                // Use youtube-dl to get a HTTP URL
                format!("urisourcebin uri={}", input)
            } else {
                format!("urisourcebin uri={}", input)
            }
        } else if Path::new(input).exists() {
            format!("filesrc location={}", input)
        } else {
            println!("Cannot open {}", input);
            process::exit(-1);
        };

    gstreamer::init().unwrap();

    let pipeline_str = format!("{} ! \
                               decodebin name=dmux \
                               dmux. ! queue ! autovideoconvert ! autovideosink \
                               dmux. ! queue ! audioconvert ! autoaudiosink",
                               source);
    let mut context = gstreamer::ParseContext::new();
    let pipeline =
        match gstreamer::parse_launch_full(&pipeline_str, Some(&mut context), gstreamer::ParseFlags::empty()) {
            Ok(pipeline) => pipeline,
            Err(err) => {
                if let Some(gstreamer::ParseError::NoSuchElement) = err.kind::<gstreamer::ParseError>() {
                    println!("Missing element(s): {:?}", context.missing_elements());
                } else {
                    println!("Failed to parse pipeline: {}", err);
                }
                process::exit(-1)
            }
        };
    let bus = pipeline.bus().unwrap();

    pipeline
        .set_state(gstreamer::State::Playing)
        .expect("Unable to set the pipeline to the `Playing` state");

    println!("Pipeline playing...");

    let (tx, rx) = channel();
    ctrlc::set_handler(move || tx.send(()).expect("Could not send signal on channel.")).unwrap();

    'running: loop {
        for msg in bus.iter_timed(gstreamer::ClockTime::from_mseconds(100)) {
            use gstreamer::MessageView;

            match msg.view() {
                MessageView::Eos(..) => break 'running,
                MessageView::Error(err) => {
                    println!(
                        "Error from {:?}: {} ({:?})",
                        err.src().map(|s| s.path_string()),
                        err.error(),
                        err.debug()
                    );
                    break 'running;
                }
                _ => (),
            }
        }

        if let Ok(()) = rx.recv_timeout(Duration::from_millis(100)) {
            println!("User-requested stop");
            break 'running;
        };
    };

    // Shutdown pipeline
    pipeline
        .set_state(gstreamer::State::Null)
        .expect("Unable to set the pipeline to the `Null` state");

    println!("Pipeline stopped...");
}
