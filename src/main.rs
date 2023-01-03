use std::env;
use std::path::Path;
use std::process;
use url::Url;
use sdl2::pixels::PixelFormatEnum;
use gstreamer::prelude::*;

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;

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

    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    let mut event_pump = sdl_context.event_pump().unwrap();

    let window = video_subsystem.window(&args[0], WIDTH, HEIGHT)
        .position_centered()
        .resizable()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .unwrap();

    // Because every Texture is owned by a TextureCreator, we need
    // to own the TextureCreature, to prevent its drop.
    let texture_creator = canvas.texture_creator();

    gstreamer::init().unwrap();

    let pipeline_str = format!("{} ! \
                               decodebin name=dmux \
                               dmux. ! queue ! autovideoconvert ! video/x-raw,format=I420 ! appsink name=sink sync=true \
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

    let pipeline = pipeline.dynamic_cast::<gstreamer::Pipeline>().unwrap();
    let sink = pipeline.by_name("sink").unwrap();
    let appsink = sink.dynamic_cast::<gstreamer_app::AppSink>().unwrap();

    pipeline
        .set_state(gstreamer::State::Playing)
        .expect("Unable to set the pipeline to the `Playing` state");

    println!("Pipeline playing...");

    let bus = pipeline.bus().unwrap();
    let mut playing = true;
    let mut width = WIDTH;
    let mut height = HEIGHT;
    let mut tex = texture_creator.create_texture_streaming(PixelFormatEnum::IYUV, width, height).unwrap();

    'running: loop {
        for msg in bus.iter() {
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

        for event in event_pump.poll_iter() {
            use sdl2::event::Event;
            use sdl2::keyboard::Keycode;

            match event {
                Event::Quit {..} |
                Event::KeyDown { keycode: Some(Keycode::Q), .. } |
                Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                    break 'running
                },
                Event::KeyDown { keycode: Some(Keycode::Space), .. } => {
                    if playing {
                        playing = false;
                        pipeline
                        .set_state(gstreamer::State::Paused)
                        .expect("Unable to set the pipeline to the `Paused` state");
                        println!("Pipeline paused...");
                    } else {
                        playing = true;
                        pipeline
                        .set_state(gstreamer::State::Playing)
                        .expect("Unable to set the pipeline to the `Playing` state");
                        println!("Pipeline playing...");
                    }
                },
                _ => {}
            }
        }

        if !playing {
            continue 'running;
        }

        match appsink.try_pull_sample(gstreamer::ClockTime::from_mseconds(40)) {
            Some(sample) => {
                let buffer = sample.buffer().unwrap();
                let caps = sample.caps().expect("Sample without caps");
                let info = gstreamer_video::VideoInfo::from_caps(caps).expect("Failed to parse caps");
                let frame = gstreamer_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info).unwrap();

                if frame.width() != width || frame.height() != height {
                    println!("Video negotiated {}x{}", frame.width(), frame.height());
                    println!("{} planes", frame.n_planes());

                    width = frame.width();
                    height = frame.height();
                    tex = texture_creator.create_texture_streaming(PixelFormatEnum::IYUV, width, height).unwrap();
                }

                if width > 0 && height > 0 {
                    tex.update_yuv(None,
                                   frame.plane_data(0).unwrap(),
                                   frame.plane_stride()[0] as usize,
                                   frame.plane_data(1).unwrap(),
                                   frame.plane_stride()[1] as usize,
                                   frame.plane_data(2).unwrap(),
                                   frame.plane_stride()[2] as usize)
                        .unwrap();
                    canvas.clear();
                    canvas.copy(&tex, None, None).unwrap();
                    canvas.present();
                }
            },
            None => {
                if appsink.is_eos() {
                    break 'running;
                }
            },
        };
    };

    // Shutdown pipeline
    pipeline
        .set_state(gstreamer::State::Null)
        .expect("Unable to set the pipeline to the `Null` state");

    println!("Pipeline stopped...");
}
