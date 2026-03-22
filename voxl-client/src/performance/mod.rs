use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Timing statistics for a single frame
#[derive(Clone, Debug, Default)]
pub struct FrameTiming {
    pub total_frame_time: Duration,     // Complete frame time
    pub cpu_time: Duration,             // Main thread CPU time
    pub gpu_submission_time: Duration,  // Estimated GPU submission time
    pub ecs_systems_time: Duration,     // All ECS systems combined
    pub mesh_processing_time: Duration, // Mesh update processing
}

/// Rolling statistics (average, min, max)
#[derive(Clone, Debug)]
pub struct RollingStats {
    pub avg: Duration,
    pub min: Duration,
    pub max: Duration,
    pub samples: u32,
}

impl Default for RollingStats {
    fn default() -> Self {
        Self {
            avg: Duration::ZERO,
            min: Duration::MAX,
            max: Duration::ZERO,
            samples: 0,
        }
    }
}

impl RollingStats {
    /// Update statistics with a new sample
    pub fn update(&mut self, duration: Duration) {
        self.samples += 1;

        // Update min/max
        if duration < self.min {
            self.min = duration;
        }
        if duration > self.max {
            self.max = duration;
        }

        // Update average (running average)
        let total = self.avg.as_nanos() as u128 * (self.samples - 1) as u128
            + duration.as_nanos() as u128;
        self.avg = Duration::from_nanos((total / self.samples as u128) as u64);
    }

    /// Get formatted string for display
    pub fn as_ms(&self) -> f64 {
        self.avg.as_secs_f64() * 1000.0
    }
}

/// Memory usage statistics
#[derive(Clone, Debug)]
pub struct MemoryStats {
    pub loaded_chunks: usize,
    pub loaded_meshes: usize,
    pub pending_mesh_requests: usize,
    pub voxel_memory_mb: f64,
    pub mesh_memory_mb: f64,
}

impl Default for MemoryStats {
    fn default() -> Self {
        Self {
            loaded_chunks: 0,
            loaded_meshes: 0,
            pending_mesh_requests: 0,
            voxel_memory_mb: 0.0,
            mesh_memory_mb: 0.0,
        }
    }
}

/// Per-system timing breakdown
#[derive(Clone, Debug)]
pub struct SystemTiming {
    pub name: String,
    pub duration: Duration,
}

/// Complete performance snapshot for UI display
#[derive(Clone, Debug)]
pub struct PerformanceSnapshot {
    pub fps: f32,
    pub frame_timing: RollingStats,
    pub cpu_timing: RollingStats,
    pub gpu_timing: RollingStats,
    pub system_timings: HashMap<String, RollingStats>,
    pub memory: MemoryStats,
    pub timestamp: Instant,
}

impl Default for PerformanceSnapshot {
    fn default() -> Self {
        Self {
            fps: 0.0,
            frame_timing: RollingStats::default(),
            cpu_timing: RollingStats::default(),
            gpu_timing: RollingStats::default(),
            system_timings: HashMap::new(),
            memory: MemoryStats::default(),
            timestamp: Instant::now(),
        }
    }
}

/// Central performance stats collector
///
/// This collector gathers timing data from different parts of the code
/// and maintains rolling statistics for display in the debug UI.
pub struct PerformanceCollector {
    // Frame timing ring buffer (keeps last 60 frames)
    frame_times: VecDeque<Duration>,

    // Current frame timing being built
    current_frame_start: Option<Instant>,
    current_frame_timing: Option<FrameTiming>,

    // Rolling statistics
    frame_stats: RollingStats,
    cpu_stats: RollingStats,
    gpu_stats: RollingStats,

    // Per-system timing collectors
    system_timings: HashMap<String, RollingStats>,

    // Memory statistics
    memory: MemoryStats,

    // Update interval for UI (0.5s)
    last_ui_update: Instant,
    ui_update_interval: Duration,

    // Thread-safe snapshot for UI reads
    snapshot: Arc<RwLock<PerformanceSnapshot>>,
}

impl Default for PerformanceCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceCollector {
    /// Create a new performance collector
    pub fn new() -> Self {
        Self {
            frame_times: VecDeque::with_capacity(60),
            current_frame_start: None,
            current_frame_timing: None,
            frame_stats: RollingStats::default(),
            cpu_stats: RollingStats::default(),
            gpu_stats: RollingStats::default(),
            system_timings: HashMap::new(),
            memory: MemoryStats::default(),
            last_ui_update: Instant::now(),
            ui_update_interval: Duration::from_millis(500),
            snapshot: Arc::new(RwLock::new(PerformanceSnapshot::default())),
        }
    }

    /// Called at start of each frame
    pub fn begin_frame(&mut self) {
        self.current_frame_start = Some(Instant::now());
        self.current_frame_timing = Some(FrameTiming::default());
    }

    /// Called at end of each frame
    pub fn end_frame(&mut self) {
        if let Some(frame_start) = self.current_frame_start.take() {
            let total_duration = frame_start.elapsed();

            // Update frame statistics
            self.frame_stats.update(total_duration);

            // Update ring buffer
            self.frame_times.push_back(total_duration);
            if self.frame_times.len() > 60 {
                self.frame_times.pop_front();
            }

            // Update snapshot if enough time has passed
            let now = Instant::now();
            if now.duration_since(self.last_ui_update) >= self.ui_update_interval {
                self.update_snapshot();
                self.last_ui_update = now;
            }
        }
    }

    /// Record CPU time for a specific part of the frame
    pub fn record_cpu_time(&mut self, duration: Duration) {
        if let Some(ref mut timing) = self.current_frame_timing {
            timing.cpu_time = duration;
        }
        self.cpu_stats.update(duration);
    }

    /// Record GPU submission time
    pub fn record_gpu_time(&mut self, duration: Duration) {
        if let Some(ref mut timing) = self.current_frame_timing {
            timing.gpu_submission_time = duration;
        }
        self.gpu_stats.update(duration);
    }

    /// Record ECS system timing
    pub fn record_system_time(&mut self, system_name: &str, duration: Duration) {
        if let Some(ref mut timing) = self.current_frame_timing {
            timing.ecs_systems_time += duration;
        }

        let entry = self
            .system_timings
            .entry(system_name.to_string())
            .or_default();
        entry.update(duration);
    }

    /// Record mesh processing time
    pub fn record_mesh_processing_time(&mut self, duration: Duration) {
        if let Some(ref mut timing) = self.current_frame_timing {
            timing.mesh_processing_time = duration;
        }
    }

    /// Update memory statistics
    pub fn update_memory_stats<F>(&mut self, f: F)
    where
        F: FnOnce() -> MemoryStats,
    {
        self.memory = f();
    }

    /// Get the latest snapshot for UI display
    pub fn snapshot(&self) -> PerformanceSnapshot {
        self.snapshot
            .read()
            .unwrap()
            .clone()
    }

    /// Get shared snapshot handle (for external access)
    pub fn snapshot_handle(&self) -> Arc<RwLock<PerformanceSnapshot>> {
        Arc::clone(&self.snapshot)
    }

    /// Update the snapshot with current statistics
    fn update_snapshot(&mut self) {
        let snapshot = PerformanceSnapshot {
            fps: if self.frame_stats.avg.as_nanos() > 0 {
                1_000_000_000.0 / self.frame_stats.avg.as_nanos() as f32
            } else {
                0.0
            },
            frame_timing: self.frame_stats.clone(),
            cpu_timing: self.cpu_stats.clone(),
            gpu_timing: self.gpu_stats.clone(),
            system_timings: self.system_timings.clone(),
            memory: self.memory.clone(),
            timestamp: Instant::now(),
        };

        *self.snapshot.write().unwrap() = snapshot;
    }

    /// Get current FPS estimate
    pub fn fps(&self) -> f32 {
        if self.frame_stats.avg.as_nanos() > 0 {
            1_000_000_000.0 / self.frame_stats.avg.as_nanos() as f32
        } else {
            0.0
        }
    }
}
