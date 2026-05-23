#![no_std]

use imu_core::Quaternion;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub enum FusionConvention {
    Nwu = 0,
    Enu = 1,
    Ned = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FusionVector {
    pub axis: FusionAxes,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FusionAxes {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FusionQuaternion {
    pub element: FusionQuaternionElements,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FusionQuaternionElements {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FusionAhrsSettings {
    pub convention: FusionConvention,
    pub gain: f32,
    pub gyroscope_range: f32,
    pub acceleration_rejection: f32,
    pub magnetic_rejection: f32,
    pub recovery_trigger_period: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FusionAhrs {
    pub settings: FusionAhrsSettings,
    pub quaternion: FusionQuaternion,
    pub accelerometer: FusionVector,
    pub initialising: bool,
    pub ramped_gain: f32,
    pub ramped_gain_step: f32,
    pub angular_rate_recovery: bool,
    pub half_accelerometer_feedback: FusionVector,
    pub half_magnetometer_feedback: FusionVector,
    pub accelerometer_ignored: bool,
    pub acceleration_recovery_trigger: i32,
    pub acceleration_recovery_timeout: i32,
    pub magnetometer_ignored: bool,
    pub magnetic_recovery_trigger: i32,
    pub magnetic_recovery_timeout: i32,
}

unsafe extern "C" {
    fn FusionAhrsInitialise(ahrs: *mut FusionAhrs);
    fn FusionAhrsSetSettings(ahrs: *mut FusionAhrs, settings: *const FusionAhrsSettings);
    fn FusionAhrsReset(ahrs: *mut FusionAhrs);
    fn FusionAhrsUpdateNoMagnetometer(
        ahrs: *mut FusionAhrs,
        gyroscope: FusionVector,
        accelerometer: FusionVector,
        delta_time: f32,
    );
    fn FusionAhrsGetQuaternion(ahrs: *const FusionAhrs) -> FusionQuaternion;
}

#[derive(Clone, Copy, Debug)]
pub struct FusionFilterSettings {
    pub convention: FusionConvention,
    pub gain: f32,
    pub gyroscope_range_dps: f32,
    pub acceleration_rejection: f32,
    pub magnetic_rejection: f32,
    pub recovery_trigger_period: u32,
}

impl Default for FusionFilterSettings {
    fn default() -> Self {
        Self {
            convention: FusionConvention::Nwu,
            gain: 6.0,
            gyroscope_range_dps: 2000.0,
            acceleration_rejection: 10.0,
            magnetic_rejection: 10.0,
            recovery_trigger_period: 0,
        }
    }
}

pub struct FusionFilter {
    ahrs: FusionAhrs,
}

impl FusionFilter {
    pub fn new(settings: FusionFilterSettings) -> Self {
        let mut ahrs = unsafe { core::mem::zeroed::<FusionAhrs>() };
        unsafe {
            FusionAhrsInitialise(&mut ahrs);
            let c_settings = FusionAhrsSettings {
                convention: settings.convention,
                gain: settings.gain,
                gyroscope_range: settings.gyroscope_range_dps,
                acceleration_rejection: settings.acceleration_rejection,
                magnetic_rejection: settings.magnetic_rejection,
                recovery_trigger_period: settings.recovery_trigger_period,
            };
            FusionAhrsSetSettings(&mut ahrs, &c_settings);
        }
        Self { ahrs }
    }

    pub fn reset(&mut self) {
        unsafe { FusionAhrsReset(&mut self.ahrs) };
    }

    pub fn update_imu(
        &mut self,
        accel_ms2: [f32; 3],
        gyro_rads: [f32; 3],
        dt_s: f32,
    ) -> Quaternion {
        let accelerometer = FusionVector {
            axis: FusionAxes {
                x: accel_ms2[0],
                y: accel_ms2[1],
                z: accel_ms2[2],
            },
        };
        let gyroscope = FusionVector {
            axis: FusionAxes {
                x: gyro_rads[0],
                y: gyro_rads[1],
                z: gyro_rads[2],
            },
        };

        unsafe {
            FusionAhrsUpdateNoMagnetometer(&mut self.ahrs, gyroscope, accelerometer, dt_s);
            let q = FusionAhrsGetQuaternion(&self.ahrs);
            Quaternion {
                w: q.element.w,
                x: q.element.x,
                y: q.element.y,
                z: q.element.z,
            }
        }
    }
}
