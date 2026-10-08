//! Native counters are available only for the local macOS transport.
use super::{CpuCoreCounter, CpuCounter, CpuSample};
use anyhow::{Result, ensure};
use std::{mem::size_of, ptr};

const MAX_PROCESSORS: usize = 65536;
const WORDS_PER_PROCESSOR: usize = libc::CPU_STATE_MAX as usize;
const NULL_PORT: libc::mach_port_t = libc::MACH_PORT_NULL as libc::mach_port_t;

// libc exposes the processor/VM calls but not this SDK function. Both arguments
// are Mach port names (u32); the result is kern_return_t (i32).
unsafe extern "C" {
    fn mach_port_deallocate(
        task: libc::mach_port_t,
        name: libc::mach_port_t,
    ) -> libc::kern_return_t;
}

struct HostPort(libc::mach_port_t);

// The SDK's mach_task_self macro reads this borrowed process-global port.
// libc keeps the equivalent typed wrapper deprecated in favor of mach2;
// a separate Mach binding crate is unnecessary for this small collector.
#[allow(deprecated)]
fn task_port() -> libc::mach_port_t {
    // SAFETY: libc's wrapper reads the process-global task self port.
    unsafe { libc::mach_task_self() }
}

impl Drop for HostPort {
    fn drop(&mut self) {
        if self.0 != NULL_PORT {
            // SAFETY: mach_host_self returned an owned send right. The task
            // self port is borrowed and is never itself deallocated.
            let result = unsafe { mach_port_deallocate(task_port(), self.0) };
            if result != libc::KERN_SUCCESS {
                tracing::debug!(result, "Unable to release macOS CPU host port");
            }
        }
    }
}

struct ProcessorInfo {
    words: libc::processor_info_array_t,
    count: libc::mach_msg_type_number_t,
}

impl Drop for ProcessorInfo {
    fn drop(&mut self) {
        if !self.words.is_null() {
            // count is the returned number of integer words, not CPUs or bytes.
            // A u32 word count times four fits vm_size_t on supported macOS.
            let bytes = self.count as libc::vm_size_t * size_of::<libc::integer_t>();
            // SAFETY: this is the allocation returned by host_processor_info.
            // The guard exists before validating its shape or making a slice.
            let result = unsafe {
                libc::vm_deallocate(task_port(), self.words as libc::vm_address_t, bytes)
            };
            if result != libc::KERN_SUCCESS {
                tracing::debug!(result, "Unable to release macOS CPU processor information");
            }
        }
    }
}

pub(super) fn sample_cpu() -> Result<CpuSample> {
    // SAFETY: mach_host_self takes no arguments and returns a send right.
    // libc deprecates this binding in favor of mach2, not the macOS API itself.
    #[allow(deprecated)]
    let host = HostPort(unsafe { libc::mach_host_self() });
    ensure!(host.0 != NULL_PORT, "Missing macOS CPU host port");
    let mut processors = 0;
    let mut info = ProcessorInfo {
        words: ptr::null_mut(),
        count: 0,
    };
    // SAFETY: out parameters point to initialized storage; the host right lives
    // through the call, and the returned VM allocation is owned by info's guard.
    let result = unsafe {
        libc::host_processor_info(
            host.0,
            libc::PROCESSOR_CPU_LOAD_INFO,
            &mut processors,
            &mut info.words,
            &mut info.count,
        )
    };
    ensure!(
        result == libc::KERN_SUCCESS,
        "Unable to read macOS CPU counters: Mach error {result}"
    );
    let count = validated_word_count(processors as usize, info.count as usize)?;
    ensure!(
        !info.words.is_null(),
        "Missing macOS CPU counter allocation"
    );
    // SAFETY: successful host_processor_info returns count initialized integer
    // words. Its exact bounded shape and nonnull pointer were checked above.
    let words = unsafe { std::slice::from_raw_parts(info.words, count) };
    counters_from_words(processors as usize, words)
}

fn validated_word_count(processors: usize, words: usize) -> Result<usize> {
    ensure!(
        (1..=MAX_PROCESSORS).contains(&processors),
        "Invalid native macOS CPU count"
    );
    let expected = processors * WORDS_PER_PROCESSOR;
    ensure!(words == expected, "Incomplete native macOS CPU counters");
    Ok(expected)
}

fn counters_from_words(processors: usize, words: &[libc::integer_t]) -> Result<CpuSample> {
    validated_word_count(processors, words.len())?;
    let mut aggregate = CpuCounter { total: 0, idle: 0 };
    let cores = words
        .chunks_exact(WORDS_PER_PROCESSOR)
        .enumerate()
        .map(|(index, words)| {
            // Mach transports integer_t words, but cpu_ticks are natural_t.
            // Preserve the unsigned 32-bit bit pattern before widening it.
            let ticks: [u64; WORDS_PER_PROCESSOR] =
                std::array::from_fn(|index| u64::from(words[index] as u32));
            let counter = CpuCounter {
                total: ticks.iter().sum(),
                idle: ticks[libc::CPU_STATE_IDLE as usize],
            };
            // 65536 CPUs * four u32 counters fits comfortably in u64.
            aggregate.total += counter.total;
            aggregate.idle += counter.idle;
            CpuCoreCounter {
                // Match the existing logical CPU display identifiers on Linux/Windows.
                name: index.to_string(),
                counter,
            }
        })
        .collect();
    Ok(CpuSample::Counters { aggregate, cores })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_ticks_include_nice_and_sum_real_cores() -> Result<()> {
        let sample = counters_from_words(2, &[10, 20, 30, 40, i32::MIN, -1, i32::MIN, 5])?;
        let CpuSample::Counters { aggregate, cores } = sample else {
            anyhow::bail!("Expected counters");
        };
        assert_eq!(cores[0].name, "0");
        assert_eq!(
            cores[0].counter,
            CpuCounter {
                total: 100,
                idle: 30
            }
        );
        assert_eq!(cores[1].name, "1");
        assert_eq!(
            cores[1].counter.total,
            2 * (1_u64 << 31) + u32::MAX as u64 + 5
        );
        assert_eq!(cores[1].counter.idle, 1_u64 << 31);
        assert_eq!(
            aggregate.total,
            cores.iter().map(|core| core.counter.total).sum::<u64>()
        );
        assert_eq!(aggregate.idle, 30 + (1_u64 << 31));
        Ok(())
    }

    #[test]
    fn counter_shape_is_bounded_and_exact() {
        for (processors, words) in [
            (0, 0),
            (1, 0),
            (1, 3),
            (1, 5),
            (65537, 262148),
            (usize::MAX, 0),
        ] {
            assert!(validated_word_count(processors, words).is_err());
        }
        assert_eq!(validated_word_count(65536, 262144).ok(), Some(262144));
        assert!(counters_from_words(2, &[0; 4]).is_err());
    }

    #[test]
    #[ignore = "Reads the local macOS Mach API and sysctl; run explicitly for native ABI QA"]
    fn live_cpu_snapshots_match_local_topology_and_advance() -> Result<()> {
        let output = std::process::Command::new("/usr/sbin/sysctl")
            .args(["-n", "hw.logicalcpu"])
            .output()?;
        ensure!(
            output.status.success(),
            "Unable to read local logical CPU count"
        );
        let logical_cpus = std::str::from_utf8(&output.stdout)?
            .trim()
            .parse::<usize>()?;
        let first = sample_cpu()?;
        std::thread::sleep(std::time::Duration::from_millis(150));
        let second = sample_cpu()?;
        let (
            CpuSample::Counters {
                aggregate: old,
                cores: old_cores,
            },
            CpuSample::Counters {
                aggregate: new,
                cores: new_cores,
            },
        ) = (first, second)
        else {
            anyhow::bail!("Expected native CPU counters");
        };
        assert_eq!(old_cores.len(), logical_cpus);
        assert_eq!(new_cores.len(), logical_cpus);
        let total = new
            .total
            .checked_sub(old.total)
            .ok_or_else(|| anyhow::anyhow!("CPU rolled over during live test"))?;
        let idle = new
            .idle
            .checked_sub(old.idle)
            .ok_or_else(|| anyhow::anyhow!("CPU idle rolled over during live test"))?;
        assert!(total > 0 && idle <= total);
        for (index, (old, new)) in old_cores.iter().zip(&new_cores).enumerate() {
            assert_eq!(old.name, index.to_string());
            assert_eq!(new.name, old.name);
            assert!(old.counter.idle <= old.counter.total);
            assert!(new.counter.idle <= new.counter.total);
            assert!(new.counter.total >= old.counter.total);
            assert!(new.counter.idle >= old.counter.idle);
        }
        Ok(())
    }
}
