use hsv::hsv_to_rgb;
use maths::{apply_rotation_float2, float2_add, float2_subtract, Float2, Float4};
use metal::MTLResourceOptions;
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSAnyEventMask, NSEventType};
use objc2_foundation::{NSComparisonResult, NSDate, NSDefaultRunLoopMode};
use utils::{copy_to_buf, get_library, get_next_frame, init_render_with_bufs, make_buf, new_render_pass_descriptor, prepare_pipeline_state, simple_app};
use rand::random;

mod maths;
mod utils;

fn color_convert(int_color : (u8,u8,u8)) -> Float4 {
    Float4(int_color.0 as f32 / 255.0, int_color.1 as f32 / 255.0, int_color.2 as f32 / 255.0, 1.0)
}

//  jumprope vertical
//      full screen colored band
//      maybe shifting color slightly because telegraphed
//      would be cool if particles at end to make it more substantial
//  laser horizontal
//      speed tuning is the most important thing
//      single colored, fx unclear maybe ghosting trail
//  clusterbomb thingy maybe
//      lob out then explode, circle on ground is important
//  chasers
//      following player with simple predictable pattern
//      maybe should choose direction + lunge
//      touching one with same color clears it, maybe fade out
//
//  want to decide on whether same color is safe or different color is safe
//  I think same color being safe makes sense, then also as you lose signal you have to just dodge
//  unclear what that means for the jumpropes, maybe color band is pretty lenient


const COLOR_STEPS : u32 = 12;
fn stepped_hue(t : f64) -> f64 {
    let hue_step = 360 / COLOR_STEPS;
    let hue = t * 360.0;
    let int_hue = hue as u32 / hue_step;
    (int_hue * hue_step) as f64
}

#[repr(C)]
struct Uniforms {
    screen_x : f32,
    screen_y : f32,
    radius : f32,
    last_vert : u32
}

fn rect_intersect (rect1 : &[vertex_t], rect2 : &[vertex_t]) -> bool {
    rect1[0].position.0 < rect2[1].position.0 && rect2[0].position.0 < rect1[1].position.0 && rect1[0].position.1 < rect2[2].position.1 && rect2[0].position.1 < rect1[2].position.1
}

fn main() {
    let view_width = 1024.0;
    let view_height = 768.0;
    let fps = 60.0f32;
    let mut frames = 0;
    let mut frame_time = get_next_frame(fps as f64);
    let mut keys_pressed = vec![112];

    let (app, window, device, layer) = simple_app(view_width as f64, view_height as f64, "Colorstep");

    let shaderlib = get_library(&device);

    let render_pipeline = prepare_pipeline_state(&device, "box_vertex", "box_fragment", &shaderlib);
    let goal_pipeline = prepare_pipeline_state(&device, "box_vertex", "goal_fragment", &shaderlib);
    let command_queue = device.new_command_queue();

    //player params
    let mut x = 0.0;
    let mut y = 0.0;
    let player_speed = 600.0;
    let width = 50.0;
    let height = width;
    let mut health = 5;

    let mut lerp_t = 0.0;
    let mut int_color = hsv_to_rgb(lerp_t * 360.0, 1.0, 1.0);
    let mut color = color_convert(int_color);

    // spawning target and storing color
    let goal_x = 0.0;
    let goal_y = 600.0;
    let goal_width = 100.0;
    let goal_height = 100.0;
    let goal_t = random::<f64>();
    let mut goal_color = color_convert(hsv_to_rgb(stepped_hue(goal_t), 1.0, 1.0));


    //spawning lasers
    let num_path_spawns = 5;
    let mut path_positions : Vec<Vec<Float2>> = vec![Vec::new(); num_path_spawns];
    let mut path_colors : Vec<Vec<Float4>> = vec![Vec::new(); num_path_spawns];
    let path_x = 1024.0;
    let path_width = 150.0;
    let path_height = (2.0 * view_height) / num_path_spawns as f32;
    let path_speed = 10.0 * 60.0;

    let projectile_width = 100.0;
    let projectile_height =  projectile_width / 5.0;

    for i in 0..num_path_spawns {
        path_positions[i].push(Float2(path_x + (random::<f32>() * path_width / 10.0).floor() * 10.0, ((2.0 * view_height / num_path_spawns as f32) * i as f32 + path_height / 2.0) - view_height));
        path_colors[i].push(color_convert(hsv_to_rgb(stepped_hue(random::<f64>()), 1.0, 1.0)));
    }
    // redundant work, done later
    let mut vertex_data = Vec::new();
    for i in 0..path_positions.len() {
        for j in 0..path_positions[i].len() {
            vertex_data.append(&mut build_rect(path_positions[i][j].0, path_positions[i][j].1, path_width, path_height, 0.0, path_colors[i][j]));
        }
    }

    //jumprope params
    let mut accum = 0.0;
    let jumprope_spawn_threshold = 100.0;
    let jumprope_limit = 4;

    let jumprope_speed = 200.0;
    let jumprope_x = 0.0;
    let jumprope_y = view_height;
    let jumprope_width = view_width * 2.5;
    let jumprope_height = projectile_height;

    let mut jumprope_positions = Vec::new();
    let mut jumprope_ts = Vec::new();

    //spawn initial jumprope
    jumprope_positions.push(jumprope_y);
    jumprope_ts.push(random::<f64>());

    let vert_buf = device.new_buffer_with_data(
        vertex_data.as_ptr() as *const _,
        size_of::<vertex_t>() as u64 * 4 * 1024,
        MTLResourceOptions::CPUCacheModeDefaultCache | MTLResourceOptions::StorageModeManaged
    );

    let mut radius = 300.0;
    let mut signal_lost = 0.0;
    loop {
        autoreleasepool(|_| {
            if app.windows().is_empty() {
                unsafe {app.terminate(None);}
            }
            if unsafe { frame_time.compare(&NSDate::now()) } == NSComparisonResult::Ascending {
                frame_time = get_next_frame(fps as f64);
                frames += 1;

                for key in keys_pressed.iter() {
                    // println!("{key}");
                    match key {
                        0 => x -= player_speed / fps,
                        1 => y -= player_speed / fps,
                        2 => x += player_speed / fps,
                        14 => {signal_lost += 0.1 / fps; radius += 10.0 / fps},
                        13 => y += player_speed / fps,
                        _ => ()
                    }
                }

                let mut vertex_data = Vec::new();
                int_color = hsv_to_rgb(stepped_hue(lerp_t), 1.0, 1.0);
                color = color_convert(int_color);
                vertex_data.append(&mut build_rect(x, y, width, height, 0.0, color));

                //check jumprope spawn
                accum += random::<f64>();
                if accum >= jumprope_spawn_threshold  && jumprope_positions.len() < jumprope_limit {
                    jumprope_positions.push(jumprope_y);
                    jumprope_ts.push(random());
                    accum = 0.0;
                }

                //build jumprope and move by speed
                for i in 0..jumprope_positions.len() {
                    jumprope_positions[i] -= jumprope_speed / fps;
                    vertex_data.append(&mut build_rect(jumprope_x, jumprope_positions[i], jumprope_width, jumprope_height, 0.0, color_convert(hsv_to_rgb(stepped_hue(jumprope_ts[i]), 1.0, 1.0))));
                }

                //remove old jumpropes
                if jumprope_positions.first().is_some_and(|&val| val < -view_height) {
                    jumprope_positions.remove(0);
                    jumprope_ts.remove(0);
                }

                //copying new vertex data per frame, not good but whatever
                let mut path_count = 0;
                for i in 0..path_positions.len() {
                    for j in 0..path_positions[i].len() {
                        path_positions[i][j].0 -= path_speed / fps;
                        vertex_data.append(&mut build_rect(path_positions[i][j].0, path_positions[i][j].1, projectile_width, projectile_height, 0.0, path_colors[i][j]));
                        path_count += 1;
                    }
                    if path_positions[i].last().unwrap().0 < (path_width * -0.45) + path_x {
                        path_positions[i].push(Float2(path_x + (random::<f32>() * 1.5 * path_width / 10.0).floor() * 10.0, ((2.0 * view_height / num_path_spawns as f32) * i as f32 + path_height / 2.0) - view_height + (random::<f32>() - 0.5) * path_height));
                        path_colors[i].push(color_convert(hsv_to_rgb(stepped_hue(random::<f64>()), 1.0, 1.0)));
                    }
                    if path_positions[i].first().unwrap().0 < (path_width * -0.55) - path_x {
                        path_positions[i].remove(0);
                        path_colors[i].remove(0);
                    }
                }
                let last_vert = vertex_data.len() as u32 - 4;

                // let all_rects = vertex_data.iter().map(|vert| vert.position).collect::<Vec<Float4>>();
                // let (player, rest) = all_rects.split_at(4);
                // let rects = rest.chunks(4);
                // let goal_rect = rects.last();

                let (player, rest) = vertex_data.split_at_mut(4);
                let rects = rest.chunks(4);

                // let mut chunk_number = 1;
                // let mut collision = 999;
                for rect in rects {
                    if rect_intersect(player, rect) {
                        if player[0].color != rect[0].color {
                            for i in 0..4 {
                                player[i].color = Float4(1.0, 0.0, 0.0, 1.0);
                                health -= 1;
                                // collision = chunk_number
                            }
                        } else {
                            signal_lost += 0.01;
                        }
                    }
                    // chunk_number += 1;
                }
                if health == 0 {
                    println!("You lose!");
                    unsafe { app.terminate(None) };
                }
                let goal_verts = build_rect(goal_x, goal_y, goal_width, goal_height, 0.0, goal_color);
                // let goal_rect = goal_verts.iter().map(|val| val.position).collect::<Vec<Float4>>();
                if rect_intersect(player, &goal_verts) && stepped_hue(lerp_t) == stepped_hue(goal_t) {
                    println!("Goal reached!");
                }
                // if collision < 999 {
                //     // path_positions.remove(collision);
                //     // path_colors.remove(collision);
                //     for _ in 0..4 {
                //         vertex_data.remove(collision * 4);
                //     }
                // }
                copy_to_buf(&vertex_data, &vert_buf);

                let command_buffer = command_queue.new_command_buffer();

                let drawable = layer.next_drawable().unwrap();
                let texture = drawable.texture();
                let render_descriptor = new_render_pass_descriptor(&texture);

                let encoder = init_render_with_bufs(&vec![], &render_descriptor, &render_pipeline, command_buffer);
                encoder.set_vertex_bytes(0, (size_of::<Uniforms>()) as u64, vec![Uniforms{screen_x : view_width as f32, screen_y : view_height as f32, radius, last_vert}].as_ptr() as *const _);
                // encoder.set_vertex_bytes(1, (size_of::<vertex_t>() * vertex_data.len()) as u64, vertex_data.as_ptr() as *const _);
                encoder.set_vertex_buffer(1, Some(&vert_buf), 0);
                encoder.set_fragment_bytes(0, (size_of::<Uniforms>()) as u64, vec![Uniforms{screen_x : view_width as f32, screen_y : view_height as f32, radius, last_vert}].as_ptr() as *const _);
                encoder.set_fragment_bytes(1, size_of::<Float2>() as u64, vec![Float2(x, y)].as_ptr() as *const _);
                encoder.set_fragment_bytes(2, (size_of::<f32>()) as u64, vec![signal_lost].as_ptr() as *const _);
                encoder.draw_primitives(metal::MTLPrimitiveType::TriangleStrip, 0, 4);
                for i in 0..path_count + jumprope_positions.len() {
                    encoder.draw_primitives(metal::MTLPrimitiveType::TriangleStrip, (i as u64 + 1) * 4, 4);
                }

                encoder.set_render_pipeline_state(&goal_pipeline);
                encoder.set_vertex_bytes(1, (size_of::<vertex_t>() * 4) as u64, goal_verts.as_ptr() as *const _);

                // println!("{}", lerp_t - goal_t);
                encoder.set_fragment_bytes(0, size_of::<f32>() as u64, vec![(lerp_t - goal_t).abs() as f32 * 10.0].as_ptr() as *const _);
                encoder.draw_primitives(metal::MTLPrimitiveType::TriangleStrip, 0, 4);
                encoder.end_encoding();

                command_buffer.present_drawable(drawable);
                command_buffer.commit();
            }

            loop {
                unsafe {
                    let e = app.nextEventMatchingMask_untilDate_inMode_dequeue(NSAnyEventMask, None, NSDefaultRunLoopMode, true);
                    match e {
                        Some(ref e) => {
                            match e.r#type() {
                                NSEventType::MouseMoved => {
                                    lerp_t += e.deltaX() / view_width as f64;
                                    lerp_t = lerp_t.max(0.0).min(1.0);
                                    app.sendEvent(e);
                                },
                                NSEventType::KeyDown => {
                                    if !keys_pressed.contains(&e.keyCode()) {
                                        keys_pressed.push(e.keyCode());
                                    }
                                },
                                NSEventType::KeyUp => {
                                    if let Some(index) = keys_pressed.iter().position(|key| key == &e.keyCode()) {
                                        keys_pressed.remove(index);
                                    }
                                },
                                _ => app.sendEvent(e),
                            }
                        },
                        None => {
                            break;
                        }
                    }
                }
            }
        })
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
struct vertex_t {
    position : Float4,
    color : Float4,
}

fn build_rect (x : f32, y : f32, width : f32, height : f32, rot : f32, color: Float4) -> Vec<vertex_t> {
    let mut verts = Vec::new();

    let origin = Float2(x - width / 2.0, y - height / 2.0);
    let v1_pos = origin;
    let v1_rot_pos = float2_add(apply_rotation_float2(float2_subtract(v1_pos, origin), rot), origin);
    let vert1 = vertex_t{position: Float4(v1_rot_pos.0, v1_rot_pos.1, 0.0, 1.0), color};

    let v2_pos = Float2(x + width / 2.0, y - height / 2.0);
    let v2_rot_pos = float2_add(apply_rotation_float2(float2_subtract(v2_pos, origin), rot), origin);
    let vert2 = vertex_t{position: Float4(v2_rot_pos.0, v2_rot_pos.1, 0.0, 1.0), color};

    let v3_pos = Float2(x - width / 2.0, y + height / 2.0);
    let v3_rot_pos = float2_add(apply_rotation_float2(float2_subtract(v3_pos, origin), rot), origin);
    let vert3 = vertex_t{position: Float4(v3_rot_pos.0, v3_rot_pos.1, 0.0, 1.0), color};

    let v4_pos = Float2(x + width / 2.0, y + height / 2.0);
    let v4_rot_pos = float2_add(apply_rotation_float2(float2_subtract(v4_pos, origin), rot), origin);
    let vert4 = vertex_t{position: Float4(v4_rot_pos.0, v4_rot_pos.1, 0.0, 1.0), color};

    verts.push(vert1);
    verts.push(vert2);
    verts.push(vert3);
    verts.push(vert4);

    verts
}
