use crate::Quaternion;

const INITIAL_GAIN: f32 = 15.0;
const INITIALISATION_PERIOD_S: f32 = 1.0;
const GRAVITY_MS2: f32 = 9.80665;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FusionConvention {
    Nwu,
    Enu,
    Ned,
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
    settings: AhrsSettings,
    quaternion: Quaternion,
    initialising: bool,
    ramped_gain: f32,
    ramped_gain_step: f32,
    angular_rate_recovery: bool,
    half_accelerometer_feedback: Vec3,
    accelerometer_ignored: bool,
    acceleration_recovery_trigger: i32,
    acceleration_recovery_timeout: i32,
}

impl FusionFilter {
    pub fn new(settings: FusionFilterSettings) -> Self {
        let mut filter = Self {
            settings: AhrsSettings::from(settings),
            quaternion: identity_quaternion(),
            initialising: true,
            ramped_gain: INITIAL_GAIN,
            ramped_gain_step: 0.0,
            angular_rate_recovery: false,
            half_accelerometer_feedback: Vec3::ZERO,
            accelerometer_ignored: false,
            acceleration_recovery_trigger: 0,
            acceleration_recovery_timeout: 0,
        };
        filter.set_settings(settings);
        filter.reset();
        filter
    }

    pub fn reset(&mut self) {
        self.quaternion = identity_quaternion();
        self.initialising = true;
        self.ramped_gain = INITIAL_GAIN;
        self.angular_rate_recovery = false;
        self.half_accelerometer_feedback = Vec3::ZERO;
        self.accelerometer_ignored = false;
        self.acceleration_recovery_trigger = 0;
        self.acceleration_recovery_timeout = self.settings.recovery_trigger_period as i32;
    }

    pub fn update_imu(
        &mut self,
        accel_ms2: [f32; 3],
        gyro_rads: [f32; 3],
        dt_s: f32,
    ) -> Quaternion {
        if dt_s <= 0.0 {
            return self.quaternion;
        }

        let accelerometer = Vec3::new(
            accel_ms2[0] / GRAVITY_MS2,
            accel_ms2[1] / GRAVITY_MS2,
            accel_ms2[2] / GRAVITY_MS2,
        );
        let gyroscope = Vec3::new(
            gyro_rads[0].to_degrees(),
            gyro_rads[1].to_degrees(),
            gyro_rads[2].to_degrees(),
        );

        self.update_no_magnetometer(gyroscope, accelerometer, dt_s);
        self.quaternion
    }

    fn set_settings(&mut self, settings: FusionFilterSettings) {
        self.settings = AhrsSettings::from(settings);
        self.acceleration_recovery_timeout = self.settings.recovery_trigger_period as i32;
        if !self.initialising {
            self.ramped_gain = self.settings.gain;
        }
        self.ramped_gain_step = (INITIAL_GAIN - self.settings.gain) / INITIALISATION_PERIOD_S;
    }

    fn update_no_magnetometer(&mut self, gyroscope_dps: Vec3, accelerometer_g: Vec3, dt_s: f32) {
        self.update(gyroscope_dps, accelerometer_g, dt_s);

        if self.initialising {
            self.set_heading_degrees(0.0);
        }
    }

    fn update(&mut self, gyroscope_dps: Vec3, accelerometer_g: Vec3, dt_s: f32) {
        if gyroscope_dps.x.abs() > self.settings.gyroscope_range_dps
            || gyroscope_dps.y.abs() > self.settings.gyroscope_range_dps
            || gyroscope_dps.z.abs() > self.settings.gyroscope_range_dps
        {
            let quaternion = self.quaternion;
            self.reset();
            self.quaternion = quaternion;
            self.angular_rate_recovery = true;
        }

        if self.initialising {
            self.ramped_gain -= self.ramped_gain_step * dt_s;
            if self.ramped_gain < self.settings.gain || self.settings.gain == 0.0 {
                self.ramped_gain = self.settings.gain;
                self.initialising = false;
                self.angular_rate_recovery = false;
            }
        }

        let half_gravity = self.half_gravity();
        let mut half_accelerometer_feedback = Vec3::ZERO;
        self.accelerometer_ignored = true;

        if !accelerometer_g.is_zero() {
            self.half_accelerometer_feedback = feedback(accelerometer_g.normalised(), half_gravity);

            if self.initialising
                || self.half_accelerometer_feedback.magnitude_squared()
                    <= self.settings.acceleration_rejection
            {
                self.accelerometer_ignored = false;
                self.acceleration_recovery_trigger -= 9;
            } else {
                self.acceleration_recovery_trigger += 1;
            }

            if self.acceleration_recovery_trigger > self.acceleration_recovery_timeout {
                self.acceleration_recovery_timeout = 0;
                self.accelerometer_ignored = false;
            } else {
                self.acceleration_recovery_timeout = self.settings.recovery_trigger_period as i32;
            }

            self.acceleration_recovery_trigger = self
                .acceleration_recovery_trigger
                .clamp(0, self.settings.recovery_trigger_period as i32);

            if !self.accelerometer_ignored {
                half_accelerometer_feedback = self.half_accelerometer_feedback;
            }
        }

        let half_gyroscope = gyroscope_dps * (0.5_f32.to_radians());
        let adjusted_half_gyroscope =
            half_gyroscope + half_accelerometer_feedback * self.ramped_gain;

        self.quaternion = normalise_quaternion(add_quaternion(
            self.quaternion,
            multiply_quaternion_vector(self.quaternion, adjusted_half_gyroscope * dt_s),
        ));
    }

    fn half_gravity(&self) -> Vec3 {
        let q = self.quaternion;
        match self.settings.convention {
            FusionConvention::Nwu | FusionConvention::Enu => Vec3::new(
                q.x * q.z - q.w * q.y,
                q.y * q.z + q.w * q.x,
                q.w * q.w - 0.5 + q.z * q.z,
            ),
            FusionConvention::Ned => Vec3::new(
                q.w * q.y - q.x * q.z,
                -1.0 * (q.y * q.z + q.w * q.x),
                0.5 - q.w * q.w - q.z * q.z,
            ),
        }
    }

    fn set_heading_degrees(&mut self, heading_degrees: f32) {
        let q = self.quaternion;
        let yaw = libm::atan2f(q.w * q.z + q.x * q.y, 0.5 - q.y * q.y - q.z * q.z);
        let half_yaw_minus_heading = 0.5 * (yaw - heading_degrees.to_radians());
        let rotation = Quaternion {
            w: libm::cosf(half_yaw_minus_heading),
            x: 0.0,
            y: 0.0,
            z: -libm::sinf(half_yaw_minus_heading),
        };
        self.quaternion = multiply_quaternion(rotation, self.quaternion);
    }
}

#[derive(Clone, Copy, Debug)]
struct AhrsSettings {
    convention: FusionConvention,
    gain: f32,
    gyroscope_range_dps: f32,
    acceleration_rejection: f32,
    recovery_trigger_period: u32,
}

impl From<FusionFilterSettings> for AhrsSettings {
    fn from(settings: FusionFilterSettings) -> Self {
        let acceleration_rejection = if settings.acceleration_rejection == 0.0
            || settings.gain == 0.0
            || settings.recovery_trigger_period == 0
        {
            f32::MAX
        } else {
            let half_sin = 0.5 * libm::sinf(settings.acceleration_rejection.to_radians());
            half_sin * half_sin
        };

        Self {
            convention: settings.convention,
            gain: settings.gain,
            gyroscope_range_dps: if settings.gyroscope_range_dps == 0.0 {
                f32::MAX
            } else {
                0.98 * settings.gyroscope_range_dps
            },
            acceleration_rejection,
            recovery_trigger_period: settings.recovery_trigger_period,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

impl Vec3 {
    const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    fn is_zero(self) -> bool {
        self.x == 0.0 && self.y == 0.0 && self.z == 0.0
    }

    fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    fn magnitude_squared(self) -> f32 {
        self.dot(self)
    }

    fn normalised(self) -> Self {
        let magnitude_squared = self.magnitude_squared();
        if magnitude_squared <= f32::EPSILON {
            Self::ZERO
        } else {
            self * fast_inverse_sqrt(magnitude_squared)
        }
    }
}

impl core::ops::Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl core::ops::Mul<f32> for Vec3 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

fn feedback(sensor: Vec3, reference: Vec3) -> Vec3 {
    if sensor.dot(reference) < 0.0 {
        sensor.cross(reference).normalised()
    } else {
        sensor.cross(reference)
    }
}

fn identity_quaternion() -> Quaternion {
    Quaternion {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    }
}

fn add_quaternion(a: Quaternion, b: Quaternion) -> Quaternion {
    Quaternion {
        w: a.w + b.w,
        x: a.x + b.x,
        y: a.y + b.y,
        z: a.z + b.z,
    }
}

fn multiply_quaternion(a: Quaternion, b: Quaternion) -> Quaternion {
    Quaternion {
        w: a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
        x: a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
        y: a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
        z: a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
    }
}

fn multiply_quaternion_vector(q: Quaternion, v: Vec3) -> Quaternion {
    Quaternion {
        w: -q.x * v.x - q.y * v.y - q.z * v.z,
        x: q.w * v.x + q.y * v.z - q.z * v.y,
        y: q.w * v.y - q.x * v.z + q.z * v.x,
        z: q.w * v.z + q.x * v.y - q.y * v.x,
    }
}

fn normalise_quaternion(q: Quaternion) -> Quaternion {
    let magnitude_reciprocal = fast_inverse_sqrt(q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z);
    Quaternion {
        w: q.w * magnitude_reciprocal,
        x: q.x * magnitude_reciprocal,
        y: q.y * magnitude_reciprocal,
        z: q.z * magnitude_reciprocal,
    }
}

fn fast_inverse_sqrt(x: f32) -> f32 {
    let i = 0x5F1F1412u32.wrapping_sub(x.to_bits() >> 1);
    let y = f32::from_bits(i);
    y * (1.69000231 - 0.714158168 * x * y * y)
}
