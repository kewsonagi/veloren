use super::{super::Animation, ArthropodSkeleton, SkeletonAttr};
//use std::{f32::consts::PI, ops::Mul};
use super::super::vek::*;
use std::f32::consts::{FRAC_PI_2, PI};

pub struct RunAnimation;

impl Animation for RunAnimation {
    type Dependency<'a> = (Vec3<f32>, Vec3<f32>, Vec3<f32>, f32, Vec3<f32>, f32);
    type Skeleton = ArthropodSkeleton;

    #[cfg(feature = "use-dyn-lib")]
    const UPDATE_FN: &'static [u8] = b"arthropod_run\0";

    #[cfg_attr(feature = "be-dyn-lib", export_name = "arthropod_run")]
    fn update_skeleton_inner<'a>(
        skeleton: &Self::Skeleton,
        (velocity, orientation, last_ori, _global_time, avg_vel, acc_vel): Self::Dependency<'a>,
        anim_time: f32,
        rate: &mut f32,
        s_a: &SkeletonAttr,
    ) -> Self::Skeleton {
        let mut next = (*skeleton).clone();
        let speed = (Vec2::<f32>::from(velocity).magnitude()).min(22.0);
        *rate = 1.0;

        //let speednorm = speed / 13.0;
        let speednorm = (speed / 13.0).powf(0.25);
        let mixed_vel = acc_vel + anim_time * 6.0; //sets run frequency using speed, with anim_time setting a floor

        let speedmult = 1.0;
        let lab: f32 = 0.6; //6

        let short = ((1.0
            / (0.72
                + 0.28 * ((mixed_vel * 1.0 * lab * speedmult + PI * -0.15 - 0.5).sin()).powi(2)))
        .sqrt())
            * ((mixed_vel * 1.0 * lab * speedmult + PI * -0.15 - 0.5).sin())
            * speednorm;

        //
        let shortalt = (mixed_vel * 1.0 * lab * speedmult + PI * 3.0 / 8.0 - 0.5).sin() * speednorm;

        //FL
        let foot1a = (mixed_vel * 1.0 * lab * speedmult + 0.0 + PI).sin() * speednorm; //1.5
        let foot1b = (mixed_vel * 1.0 * lab * speedmult + FRAC_PI_2 + PI).sin() * speednorm; //1.9
        //FR
        let foot2a = (mixed_vel * 1.0 * lab * speedmult).sin() * speednorm; //1.2
        let foot2b = (mixed_vel * 1.0 * lab * speedmult + FRAC_PI_2).sin() * speednorm; //1.6
        let ori: Vec2<f32> = Vec2::from(orientation);
        let last_ori = Vec2::from(last_ori);
        let tilt = if ::vek::Vec2::new(ori, last_ori)
            .map(|o| o.magnitude_squared())
            .map(|m| m > 0.001 && m.is_finite())
            .reduce_and()
            && ori.angle_between(last_ori).is_finite()
        {
            ori.angle_between(last_ori).min(0.2)
                * last_ori.determine_side(Vec2::zero(), ori).signum()
        } else {
            0.0
        } * 1.3;
        let x_tilt = avg_vel.z.atan2(avg_vel.xy().magnitude()) * speednorm;

        next.chest.scale = Vec3::one() / s_a.scaler;

        next.head.position = Vec3::new(0.0, s_a.head.0, s_a.head.1);

        next.chest.position = Vec3::new(0.0, s_a.chest.0, s_a.chest.1);

        next.mandible_l.position = Vec3::new(-s_a.mandible_l.0, s_a.mandible_l.1, s_a.mandible_l.2);
        next.mandible_r.position = Vec3::new(s_a.mandible_r.0, s_a.mandible_r.1, s_a.mandible_r.2);

        next.wing_fl.position = Vec3::new(-s_a.wing_fl.0, s_a.wing_fl.1, s_a.wing_fl.2);
        next.wing_fr.position = Vec3::new(s_a.wing_fr.0, s_a.wing_fr.1, s_a.wing_fr.2);

        next.wing_bl.position = Vec3::new(-s_a.wing_bl.0, s_a.wing_bl.1, s_a.wing_bl.2);
        next.wing_br.position = Vec3::new(s_a.wing_br.0, s_a.wing_br.1, s_a.wing_br.2);

        next.leg_fl.position = Vec3::new(-s_a.leg_fl.0, s_a.leg_fl.1, s_a.leg_fl.2);
        next.leg_fr.position = Vec3::new(s_a.leg_fr.0, s_a.leg_fr.1, s_a.leg_fr.2);

        next.leg_fcl.position = Vec3::new(-s_a.leg_fcl.0, s_a.leg_fcl.1, s_a.leg_fcl.2);
        next.leg_fcr.position = Vec3::new(s_a.leg_fcr.0, s_a.leg_fcr.1, s_a.leg_fcr.2);

        next.leg_bcl.position = Vec3::new(-s_a.leg_bcl.0, s_a.leg_bcl.1, s_a.leg_bcl.2);
        next.leg_bcr.position = Vec3::new(s_a.leg_bcr.0, s_a.leg_bcr.1, s_a.leg_bcr.2);

        next.leg_bl.position = Vec3::new(-s_a.leg_bl.0, s_a.leg_bl.1, s_a.leg_bl.2);
        next.leg_br.position = Vec3::new(s_a.leg_br.0, s_a.leg_br.1, s_a.leg_br.2);

        next
    }
}
