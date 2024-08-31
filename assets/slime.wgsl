@group(0) @binding(0)
var<uniform> params: Params;
struct Params {
    speed: f32,
    turn_speed: f32,
    sensor_size: i32,
    sensor_offset_distance: f32,
    sensor_angle_offset: f32,
    fade_speed: f32,
}

@group(0) @binding(1)
var texture: texture_storage_2d<rgba8unorm, read_write>;

struct Agent {
    position: vec2<f32>,
    angle: f32,
    species: u32,
}

@group(0) @binding(2)
var<storage, read_write> agents: array<Agent>;

@group(0) @binding(3)
var<uniform> delta_seconds: f32;



fn hash(value: u32) -> u32 {
    var state = value;
    state = state ^ 2747636419u;
    state = state * 2654435769u;
    state = state ^ state >> 16u;
    state = state * 2654435769u;
    state = state ^ state >> 16u;
    state = state * 2654435769u;
    return state;
}

fn random0to1(value: u32) -> f32 {
    return f32(hash(value)) / 4294967295.0;
}

fn sense(agent: Agent, angle_offset: f32) -> f32 {
    let width = i32(textureDimensions(texture).x);
    let height = i32(textureDimensions(texture).y);
    
    let angle = agent.angle + angle_offset;
    let direction = vec2<f32>(cos(angle), sin(angle));    
    let center = vec2<i32>(agent.position + direction * params.sensor_offset_distance);

    var weight = 0.;
    for (var x = -params.sensor_size; x <= params.sensor_size; x++) {
        for (var y = -params.sensor_size; y <= params.sensor_size; y++) {
            let position = center + vec2<i32>(x, y);

            if position.x > 0 && position.x < width && position.y > 0 && position.y < height {

                if agent.species == 0 {
                    weight += textureLoad(texture, position).x;
                    weight -= textureLoad(texture, position).y;
                    weight -= textureLoad(texture, position).z;
                } else if agent.species == 1 {
                    weight -= textureLoad(texture, position).x;
                    weight += textureLoad(texture, position).y;
                    weight -= textureLoad(texture, position).z;
                } else if agent.species == 2 {
                    weight -= textureLoad(texture, position).x;
                    weight -= textureLoad(texture, position).y;
                    weight += textureLoad(texture, position).z;
                }
            }
        }
    }
    return weight;
}

@compute @workgroup_size(32, 1, 1)
fn init(@builtin(global_invocation_id) global_invocation_id: vec3<u32>, @builtin(num_workgroups) num_workgroups: vec3<u32>) {
    let location = vec2<i32>(i32(global_invocation_id.x), i32(global_invocation_id.y));
    let index = global_invocation_id.x;
    var agent = agents[index];
    var random_number = hash(global_invocation_id.y << 16u | global_invocation_id.x);
}

@compute @workgroup_size(32, 1, 1)
fn update_agents(@builtin(global_invocation_id) global_invocation_id: vec3<u32>, @builtin(num_workgroups) num_workgroups: vec3<u32>) {
    let index = global_invocation_id.x;
    let agent = &agents[index];
    var seed = hash(global_invocation_id.x + u32((*agent).position.y + (delta_seconds * 99999.)));

    // let x = i32(random_number * f32(textureDimensions(texture).x));
    // random_number = randomFloat(u32(random_number*99999));
    // let y = i32(random_number * f32(textureDimensions(texture).y));

    let width = f32(textureDimensions(texture).x);
    let height = f32(textureDimensions(texture).y);

    let front_weight = sense(*agent, 0.);
    let left_weight = sense(*agent, -params.sensor_angle_offset);
    let right_weight = sense(*agent, params.sensor_angle_offset);

    seed = hash(seed);
    let random_turn_strength = random0to1(seed);

    // if front_weight < left_weight && front_weight < right_weight {
    //     // (*agent).angle += (random_turn_strength - 0.5) / 4. * params.turn_speed * delta_seconds;
    // } else if left_weight < right_weight {
    //     (*agent).angle += params.turn_speed * delta_seconds;
    // } else if left_weight > right_weight {
    //     (*agent).angle -=  params.turn_speed * delta_seconds;
    // }    

    if front_weight < left_weight && front_weight < right_weight {
        (*agent).angle += ((right_weight - left_weight)) * params.turn_speed * delta_seconds;
    }

    var direction = vec2<f32>(cos((*agent).angle),sin((*agent).angle));

    var new_position = (*agent).position + (direction * params.speed * delta_seconds);

    seed = hash(seed);
    let random_angle = random0to1(seed);

    if new_position.x < 0 || new_position.x >= width || new_position.y < 0 || new_position.y >= height {
        (*agent).angle = (random_angle * radians(360.)) - radians(180.);
        new_position.x = clamp(new_position.x, 0., width);
        new_position.y = clamp(new_position.y, 0., height);
    }

    (*agent).position = new_position;

    storageBarrier();
    // textureStore(texture, vec2<i32>((*agent).position), vec4<f32>(cos((*agent).angle), sin((*agent).angle),1., 1.));
    // textureStore(texture, vec2<i32>((*agent).position) + vec2<i32>(-1, -1), vec4<f32>(0.9, 0.9, 0.9, 1.));
    // textureStore(texture, vec2<i32>((*agent).position) + vec2<i32>(1, -1), vec4<f32>(0.9, 0.9, 0.9, 1.));
    // textureStore(texture, vec2<i32>((*agent).position) + vec2<i32>(0, -1), vec4<f32>(1.));
    // textureStore(texture, vec2<i32>((*agent).position) + vec2<i32>(-1, 0), vec4<f32>(1.));
    // textureStore(texture, vec2<i32>((*agent).position) + vec2<i32>(0, 0), vec4<f32>(1.));
    // textureStore(texture, vec2<i32>((*agent).position) + vec2<i32>(1, 0), vec4<f32>(1.));
    // textureStore(texture, vec2<i32>((*agent).position) + vec2<i32>(0, 1), vec4<f32>(1.));
    // textureStore(texture, vec2<i32>((*agent).position) + vec2<i32>(-1, 1), vec4<f32>(0.9, 0.9, 0.9, 1.));
    // textureStore(texture, vec2<i32>((*agent).position) + vec2<i32>(1, 1), vec4<f32>(0.9, 0.9, 0.9, 1.));
    // if (*agent).species == 0 {
    //     textureStore(texture, vec2<i32>((*agent).position) + vec2<i32>(0, 0), vec4<f32>(1., 0., 0., 1.));
    // } else if (*agent).species == 1 {
    //     textureStore(texture, vec2<i32>((*agent).position) + vec2<i32>(0, 0), vec4<f32>(0., 1., 0., 1.));
    // } else if (*agent).species == 2 {
    //     textureStore(texture, vec2<i32>((*agent).position) + vec2<i32>(0, 0), vec4<f32>(0., 0., 1., 1.));
    // }
    textureStore(texture, vec2<i32>((*agent).position) + vec2<i32>(0, 0), vec4<f32>(1., 1., 1., 1.));
    
}

@compute @workgroup_size(32, 32, 1)
fn update_texture(@builtin(global_invocation_id) global_invocation_id: vec3<u32>, @builtin(num_workgroups) num_workgroups: vec3<u32>) {
    let location = vec2<i32>(i32(global_invocation_id.x), i32(global_invocation_id.y));
    let average = (
        textureLoad(texture, location + vec2<i32>(-1, -1)) * 2 +
        textureLoad(texture, location + vec2<i32>(1, -1))  * 2 +
        textureLoad(texture, location + vec2<i32>(0, -1))  * 4 +
        textureLoad(texture, location + vec2<i32>(-1, 0))  * 4 +
        textureLoad(texture, location + vec2<i32>(0, 0))   * 128 +
        textureLoad(texture, location + vec2<i32>(1, 0))   * 4 +
        textureLoad(texture, location + vec2<i32>(0, 1))   * 4 +
        textureLoad(texture, location + vec2<i32>(-1, 1))  * 2 +
        textureLoad(texture, location + vec2<i32>(1, 1))   * 2
    ) / ((2 * 4) + (4 * 4) + 128);
    
    // let color = textureLoad(texture, location);
     let new_color = vec4<f32>(average.x - params.fade_speed, average.y - params.fade_speed, average.z - params.fade_speed, 1.0);
    // let new_color = vec4<f32>(color.x - 0.01, color.y - 0.01, color.z - 0.01, 1.0);

    storageBarrier();
    textureStore(texture, location, new_color);
}
