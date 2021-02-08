//I'm sure there are better libraries for this (nalgebra)
//then again, I could use my own & not need a dependency.
//If I start writing more substantial programs it'd be well worth it
//also could benefit from SIMD, even in WASM (eventually)
// pub struct Vec3 {
//     pub x: f32, pub y: f32, pub z: f32
// }
// impl ops::Add<Vec3> for Vec3 {
//     fn add(self, b: Vec3) -> Vec3 {
//         Vec3{x: self.x + b.x, y: self.y + b.y, z: self.z + b.z}
//     }
// }
// impl ops::Mul<f32> for Vec3 {
//     fn mul(self, b: f32) -> Vec3 {
//         Vec3{x: b*self.x, y: b*self.y, z: b*self.z}
//     }
// }
// impl ops::Div<f32> for Vec3 {
//     fn mul(self, b: f32) -> Vec3 {
//         Vec3{x: b/self.x, y: b/self.y, z: b/self.z}
//     }
// }
// impl Vec3 {
//     fn length(self) -> f32 {
//         let x = self.x;
//         let y = self.y;
//         let z = self.z;
//         (x*x + y*y + z*z).sqrt()
//     }
//     fn normalise(self) {
//         self / self.length
//     }
// }

use nalgebra_glm::*;

trait PVec<T> {
    fn get_v3(&self, i: usize) -> [T; 3];
}

impl PVec<usize> for Vec<usize> {
    fn get_v3(&self, i: usize) -> [usize; 3] {
        let j = i*3;
        [self[j], self[j+1], self[j+2]]
    }
}
impl PVec<f32> for Vec<f32> {
    fn get_v3(&self, i: usize) -> [f32; 3] {
        let j = i*3;
        [self[j], self[j+1], self[j+2]]
    }
}

// I wonder what good ways exist to do this.
// pub fn vvec3_to_f32(v: &Vec<Vec3>) -> Vec<f32> {
//     let mut copy: Vec<f32> = Vec::with_capacity(v.len());
//     for n in 0..v.len() {
//         let t = v[n];
//         copy.append(&mut vec![t.x, t.y, t.z]);
//     }
//     copy
// }

pub fn compute_normals(coordinates: &Vec<Vec3>, triangles: &Vec<usize>) -> Vec<Vec3> {
    let mut normals: Vec<Vec3> = vec![vec3(0.,0.,0.); coordinates.len()];
    for i in 0..triangles.len()/3 {
        let t = triangles.get_v3(i);
        let a = &coordinates[t[0]];
        let b = &coordinates[t[1]];
        let c = &coordinates[t[2]];
        let normal = cross::<f32, U3>(&(c-b), &(a-b));
        for p in &t {
            normals[*p] += normal;
        }
    }
    for n in normals.iter_mut() {
        n.normalize_mut();
    }
    normals
}
