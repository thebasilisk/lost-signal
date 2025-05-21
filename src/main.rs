use std::f32::consts::PI;

use hsv::hsv_to_rgb;
use maths::{Float2, Float4, apply_rotation_float2, float2_add, float2_subtract, scale2};
use metal::MTLResourceOptions;
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSAnyEventMask, NSEventType};
use objc2_foundation::{NSComparisonResult, NSDate, NSDefaultRunLoopMode};
use rand::random;
use utils::{
    copy_to_buf, get_library, get_next_frame, init_render_with_bufs, new_render_pass_descriptor,
    prepare_pipeline_state, simple_app,
};

mod maths;
mod utils;

fn color_convert(int_color: (u8, u8, u8)) -> Float4 {
    Float4(
        int_color.0 as f32 / 255.0,
        int_color.1 as f32 / 255.0,
        int_color.2 as f32 / 255.0,
        1.0,
    )
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
//          to do parabolic path, calculate offset from axis aligned parabola equation then apply rotation
//  chasers
//      following player with simple predictable pattern
//      maybe should choose direction + lunge
//      touching one with same color clears it, maybe fade out
//
//  want to decide on whether same color is safe or different color is safe
//  I think same color being safe makes sense, then also as you lose signal you have to just dodge
//  unclear what that means for the jumpropes, maybe color band is pretty lenient
//
// check out oklab color gradient
//
//  stretch ideas
//      jumpropes represent game music and are the waveform with strong aura/glow
//      music augmentation based on the player life
//      interesting environmental barriers like maybe mirrors or colored walls
//          player should be able to create / interact with these walls somehow
//          being static parts of the environment would be kind of dull
//  boss ideas
//      cool 3d visuals using z axis for stuff
//      gravity bending projectiles and causing simultaneous redshift
//
//

const COLOR_STEPS: u32 = 7;
fn stepped_hue(t: f64) -> f64 {
    let hue_step = 360 / COLOR_STEPS;
    let hue = t * 360.0 - 20.0;
    let int_hue = hue as u32 / hue_step;
    (int_hue * hue_step) as f64
}

#[repr(C)]
struct Uniforms {
    screen_x: f32,
    screen_y: f32,
    radius: f32,
}

fn rect_intersect(rect1: &[vertex_t], rect2: &[vertex_t]) -> bool {
    rect1[0].position.0 < rect2[1].position.0
        && rect2[0].position.0 < rect1[1].position.0
        && rect1[0].position.1 < rect2[2].position.1
        && rect2[0].position.1 < rect1[2].position.1
}

struct Particle {
    position: Float2,
    velocity: Float2,
    acceleration: Float2,
    color: Float4,
    lifetime: f32,
}

impl Particle {
    fn spawn(
        location: Float2,
        max_velocity: f32,
        max_accel: f32,
        velocity_bias: Float2,
        color: Float4,
    ) -> Self {
        let v_theta = random::<f32>() * 2.0 * PI;
        let a_theta = random::<f32>() * 2.0 * PI;
        Particle {
            position: Float2(
                location.0 * (1.0 + random::<f32>() * 0.01 - 0.005),
                location.1 * (1.0 + random::<f32>() * 0.01 - 0.005),
            ),
            velocity: float2_add(
                Float2(v_theta.cos() * max_velocity, v_theta.sin() * max_velocity),
                velocity_bias,
            ),
            acceleration: Float2(a_theta.cos() * max_accel, a_theta.sin() * max_accel),
            color,
            lifetime: 1.0,
        }
    }
    fn update(&mut self) {
        self.lifetime -= random::<f32>() * 0.1;
        // self.acceleration = scale2(self.acceleration, self.lifetime);
        self.velocity = float2_add(self.acceleration, self.velocity);
        self.velocity = scale2(self.velocity, self.lifetime);
        self.position = float2_add(self.velocity, self.position);
    }
    fn update_custom(
        &mut self,
        delta_t: f32,
        forced_vel: Option<Float2>,
        friction: Option<f32>,
        accel: Option<Float2>,
    ) {
        self.lifetime -= delta_t;
        if let Some(val) = accel {
            self.velocity = float2_add(self.velocity, val);
        }
        if let Some(val) = friction {
            self.velocity = scale2(self.velocity, val);
        }
        if let Some(val) = forced_vel {
            self.position = float2_add(self.position, val);
        } else {
            self.position = float2_add(self.position, self.velocity);
        }
    }
}

const CLUSTER_END_T: f32 = 3.0;
const CLUSTER_START_SQAURE_SPEED: f32 = 1000000.0;

#[derive(Debug)]
struct Clusterbomb {
    start_pos: Float2,
    end_pos: Float2,
    x_vel: f32,
    y_vel: f32,
    y_accel: f32,
    color: Float4,
    t: f32,
}
impl Clusterbomb {
    fn new(start_pos: Float2, x_vel: f32, y_vel: f32, y_accel: f32, color: Float4) -> Self {
        let end_pos = float2_add(
            start_pos,
            Float2(
                x_vel * CLUSTER_END_T,
                (y_vel * CLUSTER_END_T) - 0.5 * (y_accel * CLUSTER_END_T.powf(2.0)),
            ),
        );
        Clusterbomb {
            start_pos,
            end_pos,
            x_vel,
            y_vel,
            y_accel,
            color,
            t: 0.0,
        }
    }
    fn update(&mut self, delta_t: f32) -> Float2 {
        self.t += delta_t;
        float2_add(
            self.start_pos,
            Float2(
                self.x_vel * self.t,
                (self.y_vel * self.t) - 0.5 * (self.y_accel * self.t.powf(2.0)),
            ),
        )
    }
    fn from_positions(start_pos: Float2, end_pos: Float2, color: Float4) -> Self {
        let pos_diff = float2_subtract(end_pos, start_pos);
        let x_vel = pos_diff.0 / CLUSTER_END_T;
        //sqrt(start_speed^2 - x^2) = + y^2
        let y_vel = (CLUSTER_START_SQAURE_SPEED - x_vel.powf(2.0)).sqrt();
        let y_accel = ((pos_diff.1 - (y_vel * CLUSTER_END_T)) * -2.0) / CLUSTER_END_T.powf(2.0);
        Clusterbomb {
            start_pos,
            end_pos,
            x_vel,
            y_vel,
            y_accel,
            color,
            t: 0.0,
        }
    }
}

fn main() {
    let view_width = 1024.0;
    let view_height = 768.0;
    let fps = 60.0f32;
    let mut frames = 0;
    let mut frame_time = get_next_frame(fps as f64);
    let mut keys_pressed = vec![112];

    let (app, window, device, layer) =
        simple_app(view_width as f64, view_height as f64, "Colorstep");

    let shaderlib = get_library(&device);

    let render_pipeline = prepare_pipeline_state(&device, "box_vertex", "box_fragment", &shaderlib);
    let target_pipeline =
        prepare_pipeline_state(&device, "box_vertex", "target_fragment", &shaderlib);
    // let goal_pipeline = prepare_pipeline_state(&device, "box_vertex", "goal_fragment", &shaderlib);
    let goal_pipeline =
        prepare_pipeline_state(&device, "box_vertex", "scorezone_fragment", &shaderlib);
    let command_queue = device.new_command_queue();

    let mut score = 0;

    //player params
    let mut x = 0.0;
    let mut y = 0.0;
    let player_speed = 600.0;
    let width = 50.0;
    let height = 50.0;

    let mut lerp_t = 0.0;
    let mut int_color = hsv_to_rgb(lerp_t * 360.0, 1.0, 1.0);
    let mut color = color_convert(int_color);

    // spawning target and storing color
    let goal_x = 0.0;
    let goal_y = 600.0;
    let goal_width = 100.0;
    let goal_height = 100.0;
    let mut goal_t = random::<f64>();
    let mut goal_color = color_convert(hsv_to_rgb(stepped_hue(goal_t), 1.0, 1.0));

    //spawning lasers
    let num_path_spawns = 10;
    let mut current_spawns = 2;
    let mut laser_positions: Vec<Vec<Float2>> = vec![Vec::new(); num_path_spawns];
    let mut laser_colors: Vec<Vec<Float4>> = vec![Vec::new(); num_path_spawns];
    let mut laser_ghosts: Vec<Particle> = Vec::new();
    let path_x = 1024.0;
    let path_width = 150.0;
    let mut path_height = (2.0 * view_height) / current_spawns as f32;
    let mut laser_speed = 450.0;
    let mut laser_trail_spawn_frames = 4;

    let projectile_width = 100.0;
    let projectile_height = projectile_width / 10.0;

    for i in 0..num_path_spawns {
        laser_positions[i].push(Float2(
            path_x + (random::<f32>() * path_width / 10.0).floor() * 10.0,
            ((2.0 * view_height / num_path_spawns as f32) * i as f32 + path_height / 2.0)
                - view_height,
        ));
        laser_colors[i].push(color_convert(hsv_to_rgb(
            stepped_hue(random::<f64>()),
            1.0,
            1.0,
        )));
    }
    // redundant work, done later
    let mut laser_verts: Vec<vertex_t> = Vec::new();
    let start_vert: Vec<vertex_t> = vec![vertex_t {
        position: Float4(0.0, 0.0, 0.0, 0.0),
        color: Float4(0.0, 0.0, 0.0, 0.0),
    }];
    for i in 0..laser_positions.len() {
        for j in 0..laser_positions[i].len() {
            laser_verts.append(&mut build_rect(
                laser_positions[i][j].0,
                laser_positions[i][j].1,
                path_width,
                path_height,
                0.0,
                laser_colors[i][j],
            ));
        }
    }

    //jumprope params
    let mut accum = 0.0;
    let jumprope_spawn_threshold = 200.0;
    let jumprope_limit = 4;

    let mut jumprope_speed = 150.0;
    let jumprope_x = 0.0;
    let jumprope_y = view_height;
    let jumprope_width = view_width * 2.5;
    let jumprope_height = projectile_height * 2.0;

    let mut jumprope_positions = Vec::new();
    let mut jumprope_ts = Vec::new();

    //spawn initial jumprope
    jumprope_positions.push(jumprope_y);
    jumprope_ts.push(random::<f64>());

    //clusterbomb params
    let cluster_spawn_start_score = 4;
    let cluster_spawn_increase_score = 8;
    let cluster_frag_count = 8;
    let cluster_width = 35.0;
    let cluster_frag_speed = 150.0;

    let mut clusters = Vec::new();
    let mut cluster_frag_particles = Vec::new();

    let laser_buf = device.new_buffer_with_data(
        laser_verts.as_ptr() as *const _,
        (size_of::<vertex_t>() * laser_verts.len() * 128) as u64,
        MTLResourceOptions::CPUCacheModeDefaultCache | MTLResourceOptions::StorageModeManaged,
    );
    let particle_buf = device.new_buffer_with_data(
        start_vert.as_ptr() as *const _,
        (size_of::<vertex_t>() * 4 * 1024) as u64,
        MTLResourceOptions::CPUCacheModeDefaultCache | MTLResourceOptions::StorageModeManaged,
    );
    let jump_buf = device.new_buffer_with_data(
        start_vert.as_ptr() as *const _,
        (size_of::<vertex_t>() * 4 * 16) as u64,
        MTLResourceOptions::CPUCacheModeDefaultCache | MTLResourceOptions::StorageModeManaged,
    );
    let cluster_buf = device.new_buffer_with_data(
        start_vert.as_ptr() as *const _,
        (size_of::<vertex_t>() * 4 * 16) as u64,
        MTLResourceOptions::CPUCacheModeDefaultCache | MTLResourceOptions::StorageModeManaged,
    );
    let cluster_frag_buf = device.new_buffer_with_data(
        start_vert.as_ptr() as *const _,
        (size_of::<vertex_t>() * 4 * 16) as u64,
        MTLResourceOptions::CPUCacheModeDefaultCache | MTLResourceOptions::StorageModeManaged,
    );
    let box_buf = device.new_buffer_with_data(
        start_vert.as_ptr() as *const _,
        (size_of::<vertex_t>() * 4 * 128) as u64,
        MTLResourceOptions::CPUCacheModeDefaultCache | MTLResourceOptions::StorageModeManaged,
    );

    let mut particles: Vec<Particle> = Vec::new();
    let particle_width = 10.0;

    let mut radius = 300.0;
    let mut signal_lost = 0.0;
    let mut carrying = false;
    loop {
        if signal_lost >= 1.15 {
            // println!("You lose!");
            // unsafe { app.terminate(None) };
            break;
        }
        autoreleasepool(|_| {
            if app.windows().is_empty() {
                unsafe {
                    app.terminate(None);
                }
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
                        14 => {
                            signal_lost += 0.1 / fps;
                            radius += 10.0 / fps
                        }
                        13 => y += player_speed / fps,
                        _ => (),
                    }
                }
                let mut laser_verts: Vec<vertex_t> = Vec::new();
                let mut jump_verts: Vec<vertex_t> = Vec::new();
                let mut cluster_verts: Vec<vertex_t> = Vec::new();
                let mut cluster_frag_verts: Vec<vertex_t> = Vec::new();
                let mut particle_verts: Vec<vertex_t> = Vec::new();
                let mut box_verts: Vec<vertex_t> = Vec::new();

                int_color = hsv_to_rgb(stepped_hue(lerp_t), 1.0, 1.0);
                color = color_convert(int_color);
                let player_rect = build_rect(x, y, width, height, 0.0, color);
                box_verts.append(&mut player_rect.clone());

                //check jumprope spawn
                accum += random::<f64>();
                if accum >= jumprope_spawn_threshold && jumprope_positions.len() < jumprope_limit {
                    jumprope_positions.push(jumprope_y);
                    jumprope_ts.push(random());
                    accum = 0.0;
                }

                //build jumprope and move by speed
                let mut jumps_to_remove = Vec::new();
                for i in 0..jumprope_positions.len() {
                    jumprope_positions[i] -= jumprope_speed / fps;
                    let jump_rect = &mut build_rect(
                        jumprope_x,
                        jumprope_positions[i],
                        jumprope_width,
                        jumprope_height,
                        0.0,
                        color_convert(hsv_to_rgb(stepped_hue(jumprope_ts[i]), 1.0, 1.0)),
                    );
                    particles.push(Particle::spawn(
                        Float2(jumprope_x + view_width, jumprope_positions[i]),
                        10.0,
                        3.0,
                        Float2(-5.0, 0.0),
                        color_convert(hsv_to_rgb(stepped_hue(jumprope_ts[i]), 1.0, 1.0)),
                    ));
                    particles.push(Particle::spawn(
                        Float2(jumprope_x + view_width, jumprope_positions[i]),
                        10.0,
                        3.0,
                        Float2(-5.0, 0.0),
                        color_convert(hsv_to_rgb(stepped_hue(jumprope_ts[i]), 1.0, 1.0)),
                    ));
                    particles.push(Particle::spawn(
                        Float2(jumprope_x - view_width, jumprope_positions[i]),
                        10.0,
                        3.0,
                        Float2(5.0, 0.0),
                        color_convert(hsv_to_rgb(stepped_hue(jumprope_ts[i]), 1.0, 1.0)),
                    ));
                    particles.push(Particle::spawn(
                        Float2(jumprope_x - view_width, jumprope_positions[i]),
                        10.0,
                        3.0,
                        Float2(5.0, 0.0),
                        color_convert(hsv_to_rgb(stepped_hue(jumprope_ts[i]), 1.0, 1.0)),
                    ));
                    if rect_intersect(&player_rect, jump_rect) {
                        if player_rect[0].color != jump_rect[0].color {
                            jumps_to_remove.insert(0, i);
                            signal_lost += 0.15;
                            continue;
                        } else {
                            signal_lost += 0.005;
                            radius += 1.0
                        }
                    }
                    jump_verts.append(jump_rect);
                }
                for i in jumps_to_remove {
                    jumprope_positions.remove(i);
                    jumprope_ts.remove(i);
                }

                //remove old jumpropes
                if jumprope_positions
                    .first()
                    .is_some_and(|&val| val < -view_height)
                {
                    jumprope_positions.remove(0);
                    jumprope_ts.remove(0);
                }

                //draw lasers
                //copying new vertex data per frame, not good but whatever
                let mut path_count = 0;
                let mut paths_to_remove = Vec::new();
                for i in 0..current_spawns {
                    for j in 0..laser_positions[i].len() {
                        laser_positions[i][j].0 -= laser_speed / fps;
                        let mut rect = build_rect(
                            laser_positions[i][j].0,
                            laser_positions[i][j].1,
                            projectile_width,
                            projectile_height,
                            0.0,
                            laser_colors[i][j],
                        );
                        if rect_intersect(&player_rect, &rect) {
                            if player_rect[0].color != rect[0].color {
                                for k in 0..4 {
                                    box_verts[k].color = Float4(1.0, 0.0, 0.0, 1.0);
                                }
                                signal_lost += 0.10;
                                paths_to_remove.insert(0, (i, j));
                            } else {
                                signal_lost += 0.005;
                                laser_verts.append(&mut rect);
                                path_count += 1;
                            }
                        } else {
                            laser_verts.append(&mut rect);
                            path_count += 1;
                        }
                        if frames % laser_trail_spawn_frames == 0 {
                            laser_ghosts.insert(
                                0,
                                Particle::spawn(
                                    laser_positions[i][j],
                                    0.0,
                                    0.0,
                                    Float2(0.0, 0.0),
                                    laser_colors[i][j],
                                ),
                            )
                        }
                    }
                    if laser_positions[i].last().unwrap().0 < (path_width * -0.45) + path_x {
                        laser_positions[i].push(Float2(
                            path_x + (random::<f32>() * 1.5 * path_width / 10.0).floor() * 10.0,
                            ((2.0 * view_height / current_spawns as f32) * i as f32
                                + path_height / 2.0)
                                - view_height
                                + (random::<f32>() - 0.5) * path_height,
                        ));
                        laser_colors[i].push(color_convert(hsv_to_rgb(
                            stepped_hue(random::<f64>()),
                            1.0,
                            1.0,
                        )));
                    }
                    if laser_positions[i].first().unwrap().0 < (path_width * -0.55) - path_x {
                        laser_positions[i].remove(0);
                        laser_colors[i].remove(0);
                    }
                }
                for (i, j) in paths_to_remove {
                    laser_positions[i].remove(j);
                    laser_colors[i].remove(j);
                }
                // let last_vert = vertex_data.len() as u32 - 4;

                if clusters.is_empty() {
                    if score >= cluster_spawn_start_score {
                        clusters.push(Clusterbomb::from_positions(
                            Float2(
                                (random::<f32>() * 2.0 - 1.0) * view_width,
                                (random::<f32>() * 2.0 - 1.0) * view_height,
                            ),
                            Float2(
                                x + random::<f32>() * view_width / 4.0,
                                y + random::<f32>() * view_height / 4.0,
                            ),
                            color_convert(hsv_to_rgb(stepped_hue(random()), 1.0, 1.0)),
                        ));
                    }
                    if score >= cluster_spawn_increase_score {
                        clusters.push(Clusterbomb::from_positions(
                            Float2(
                                (random::<f32>() * 2.0 - 1.0) * view_width,
                                (random::<f32>() * 2.0 - 1.0) * view_height,
                            ),
                            Float2(
                                x + random::<f32>() * view_width / 4.0,
                                y + random::<f32>() * view_height / 4.0,
                            ),
                            color_convert(hsv_to_rgb(stepped_hue(random()), 1.0, 1.0)),
                        ));
                    }
                    // clusters.push(Clusterbomb::new(Float2(x, y), (random::<f32>() * 2.0 - 1.0) * view_width / 2.0, random::<f32>() * view_height / 2.0, 100.0));
                }

                for bomb in clusters.iter_mut() {
                    let current_pos = bomb.update(1.0 / fps);
                    cluster_verts.append(&mut build_rect(
                        current_pos.0,
                        current_pos.1,
                        cluster_width,
                        cluster_width,
                        0.0,
                        bomb.color,
                    ));
                    box_verts.append(&mut build_rect(
                        bomb.end_pos.0,
                        bomb.end_pos.1,
                        width * 2.0,
                        height * 2.0,
                        0.0,
                        bomb.color,
                    ));
                    if bomb.t >= CLUSTER_END_T {
                        let theta_step = 2.0 * PI / cluster_frag_count as f32;
                        for i in 0..cluster_frag_count {
                            let theta = i as f32 * theta_step;
                            cluster_frag_particles.push(Particle::spawn(
                                bomb.end_pos,
                                0.0,
                                0.0,
                                Float2(
                                    theta.cos() * cluster_frag_speed,
                                    theta.sin() * cluster_frag_speed,
                                ),
                                bomb.color,
                            ));
                        }
                    }
                }
                clusters.retain(|bomb| bomb.t < CLUSTER_END_T);

                particles.retain(|particle| particle.lifetime > 0.0);

                for unit in particles.iter_mut() {
                    unit.update();
                    particle_verts.append(&mut build_rect(
                        unit.position.0,
                        unit.position.1,
                        particle_width,
                        particle_width,
                        0.0,
                        Float4(unit.color.0, unit.color.1, unit.color.2, unit.lifetime),
                    ));
                }

                for ghost in laser_ghosts.iter_mut() {
                    particle_verts.append(&mut build_rect(
                        ghost.position.0,
                        ghost.position.1,
                        projectile_width,
                        projectile_height,
                        0.0,
                        Float4(ghost.color.0, ghost.color.1, ghost.color.2, ghost.lifetime),
                    ));
                    ghost.lifetime -= 3.0 / fps;
                }
                laser_ghosts.retain(|ghost| ghost.lifetime > 0.0);

                let mut frags_to_remove = Vec::new();
                for i in 0..cluster_frag_particles.len() {
                    let frag = &mut cluster_frag_particles[i];
                    let rect = &mut build_rect(
                        frag.position.0,
                        frag.position.1,
                        cluster_width,
                        cluster_width,
                        0.0,
                        frag.color,
                    );
                    if rect_intersect(&player_rect, rect) {
                        if player_rect[0].color != rect[0].color {
                            for k in 0..4 {
                                box_verts[k].color = Float4(1.0, 0.0, 0.0, 1.0);
                            }
                            signal_lost += 0.20;
                            frags_to_remove.insert(0, i);
                        } else {
                            signal_lost += 0.005;
                            cluster_frag_verts.append(rect);
                        }
                    } else {
                        cluster_frag_verts.append(rect);
                    }
                    frag.update_custom(3.0 / fps, None, Some(0.75), None);
                }
                // println!("{}", cluster_frag_particles.len());
                for i in frags_to_remove {
                    cluster_frag_particles.remove(i);
                }
                cluster_frag_particles.retain(|frag| frag.lifetime > 0.0);

                if carrying && y < -view_height {
                    carrying = false;
                    goal_t = random();
                    goal_color = color_convert(hsv_to_rgb(stepped_hue(goal_t), 1.0, 1.0));
                    signal_lost -= 0.25;
                    laser_speed *= 1.05;
                    jumprope_speed *= 1.05;
                    if score % 2 == 0 {
                        current_spawns = (current_spawns + 1).min(num_path_spawns);
                        // laser_trail_spawn_frames -= 5;
                    }
                    path_height = (2.0 * view_height) / current_spawns as f32;
                    score += 1;
                    println!("+1");
                }
                if carrying {
                    goal_color = color_convert(hsv_to_rgb(stepped_hue(goal_t), 0.0, 1.0));
                }
                let goal_verts =
                    build_rect(goal_x, goal_y, goal_width, goal_height, 0.0, goal_color);
                // let goal_rect = goal_verts.iter().map(|val| val.position).collect::<Vec<Float4>>();
                if rect_intersect(&player_rect, &goal_verts)
                    && stepped_hue(lerp_t) == stepped_hue(goal_t)
                {
                    carrying = true;
                }
                copy_to_buf(&laser_verts, &laser_buf);
                copy_to_buf(&jump_verts, &jump_buf);
                copy_to_buf(&cluster_verts, &cluster_buf);
                copy_to_buf(&cluster_frag_verts, &cluster_frag_buf);
                copy_to_buf(&particle_verts, &particle_buf);
                copy_to_buf(&box_verts, &box_buf);

                let command_buffer = command_queue.new_command_buffer();

                let drawable = layer.next_drawable().unwrap();
                let texture = drawable.texture();
                let render_descriptor = new_render_pass_descriptor(&texture);

                let encoder = init_render_with_bufs(
                    &vec![],
                    &render_descriptor,
                    &render_pipeline,
                    command_buffer,
                );
                encoder.set_vertex_bytes(
                    0,
                    (size_of::<Uniforms>()) as u64,
                    vec![Uniforms {
                        screen_x: view_width as f32,
                        screen_y: view_height as f32,
                        radius,
                    }]
                    .as_ptr() as *const _,
                );
                // encoder.set_vertex_bytes(1, (size_of::<vertex_t>() * vertex_data.len()) as u64, vertex_data.as_ptr() as *const _);
                encoder.set_fragment_bytes(
                    0,
                    (size_of::<Uniforms>()) as u64,
                    vec![Uniforms {
                        screen_x: view_width as f32,
                        screen_y: view_height as f32,
                        radius,
                    }]
                    .as_ptr() as *const _,
                );
                encoder.set_fragment_bytes(
                    1,
                    size_of::<Float2>() as u64,
                    vec![Float2(x, y)].as_ptr() as *const _,
                );
                encoder.set_fragment_bytes(
                    2,
                    (size_of::<f32>()) as u64,
                    vec![signal_lost].as_ptr() as *const _,
                );
                // encoder.draw_primitives(metal::MTLPrimitiveType::TriangleStrip, 0, 4);
                encoder.set_vertex_buffer(1, Some(&laser_buf), 0);
                for i in 0..laser_verts.len() / 4 {
                    encoder.draw_primitives(
                        metal::MTLPrimitiveType::TriangleStrip,
                        (i as u64) * 4,
                        4,
                    );
                }
                encoder.set_vertex_buffer(1, Some(&jump_buf), 0);
                for i in 0..jump_verts.len() / 4 {
                    encoder.draw_primitives(
                        metal::MTLPrimitiveType::TriangleStrip,
                        (i as u64) * 4,
                        4,
                    );
                }
                encoder.set_vertex_buffer(1, Some(&cluster_buf), 0);
                for i in 0..cluster_verts.len() / 4 {
                    encoder.draw_primitives(
                        metal::MTLPrimitiveType::TriangleStrip,
                        (i as u64) * 4,
                        4,
                    );
                }
                encoder.set_vertex_buffer(1, Some(&cluster_frag_buf), 0);
                for i in 0..cluster_frag_verts.len() / 4 {
                    encoder.draw_primitives(
                        metal::MTLPrimitiveType::TriangleStrip,
                        (i as u64) * 4,
                        4,
                    );
                }
                encoder.set_vertex_buffer(1, Some(&particle_buf), 0);
                for i in 0..particle_verts.len() / 4 {
                    encoder.draw_primitives(
                        metal::MTLPrimitiveType::TriangleStrip,
                        (i as u64) * 4,
                        4,
                    );
                }
                //player draw
                encoder.set_vertex_buffer(1, Some(&box_buf), 0);
                encoder.draw_primitives(metal::MTLPrimitiveType::TriangleStrip, 0, 4);

                //target draws
                encoder.set_render_pipeline_state(&target_pipeline);
                encoder.set_vertex_buffer(1, Some(&box_buf), 0);
                for i in 1..box_verts.len() / 4 {
                    encoder.draw_primitives(
                        metal::MTLPrimitiveType::TriangleStrip,
                        (i as u64) * 4,
                        4,
                    );
                }

                encoder.set_render_pipeline_state(&goal_pipeline);
                encoder.set_vertex_bytes(
                    1,
                    (size_of::<vertex_t>() * 4) as u64,
                    goal_verts.as_ptr() as *const _,
                );

                // println!("{}", lerp_t - goal_t);
                encoder.set_fragment_bytes(
                    0,
                    size_of::<f32>() as u64,
                    vec![(lerp_t - goal_t).abs() as f32 * 10.0].as_ptr() as *const _,
                );
                encoder.draw_primitives(metal::MTLPrimitiveType::TriangleStrip, 0, 4);
                encoder.end_encoding();

                command_buffer.present_drawable(drawable);
                command_buffer.commit();
            }

            loop {
                unsafe {
                    let e = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                        NSAnyEventMask,
                        None,
                        NSDefaultRunLoopMode,
                        true,
                    );
                    match e {
                        Some(ref e) => match e.r#type() {
                            NSEventType::MouseMoved => {
                                lerp_t = e.locationInWindow().x / view_width as f64;
                                lerp_t = lerp_t.max(0.0).min(1.0);
                                app.sendEvent(e);
                            }
                            NSEventType::KeyDown => {
                                if !keys_pressed.contains(&e.keyCode()) {
                                    keys_pressed.push(e.keyCode());
                                }
                            }
                            NSEventType::KeyUp => {
                                if let Some(index) =
                                    keys_pressed.iter().position(|key| key == &e.keyCode())
                                {
                                    keys_pressed.remove(index);
                                }
                            }
                            _ => app.sendEvent(e),
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
    position: Float4,
    color: Float4,
}

fn build_rect(x: f32, y: f32, width: f32, height: f32, rot: f32, color: Float4) -> Vec<vertex_t> {
    let mut verts = Vec::new();

    let origin = Float2(x - width / 2.0, y - height / 2.0);
    let v1_pos = origin;
    let v1_rot_pos = float2_add(
        apply_rotation_float2(float2_subtract(v1_pos, origin), rot),
        origin,
    );
    let vert1 = vertex_t {
        position: Float4(v1_rot_pos.0, v1_rot_pos.1, 0.0, 1.0),
        color,
    };

    let v2_pos = Float2(x + width / 2.0, y - height / 2.0);
    let v2_rot_pos = float2_add(
        apply_rotation_float2(float2_subtract(v2_pos, origin), rot),
        origin,
    );
    let vert2 = vertex_t {
        position: Float4(v2_rot_pos.0, v2_rot_pos.1, 0.0, 1.0),
        color,
    };

    let v3_pos = Float2(x - width / 2.0, y + height / 2.0);
    let v3_rot_pos = float2_add(
        apply_rotation_float2(float2_subtract(v3_pos, origin), rot),
        origin,
    );
    let vert3 = vertex_t {
        position: Float4(v3_rot_pos.0, v3_rot_pos.1, 0.0, 1.0),
        color,
    };

    let v4_pos = Float2(x + width / 2.0, y + height / 2.0);
    let v4_rot_pos = float2_add(
        apply_rotation_float2(float2_subtract(v4_pos, origin), rot),
        origin,
    );
    let vert4 = vertex_t {
        position: Float4(v4_rot_pos.0, v4_rot_pos.1, 0.0, 1.0),
        color,
    };

    verts.push(vert1);
    verts.push(vert2);
    verts.push(vert3);
    verts.push(vert4);

    verts
}
