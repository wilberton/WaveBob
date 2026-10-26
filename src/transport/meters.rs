//! Fixed atomic meter bank. The audio callback writes only scalars; allocation,
//! UUID formatting and history readback happen exclusively on the control thread.
use super::*;
use crate::metering::{Block, Column, History, Observation};
const STREAMS: usize = 128;
const COLUMNS: usize = 32;
struct Slot {
    id: [AtomicU64; 2],
    bypassed: AtomicBool,
    length: AtomicU32,
    cursor: AtomicU32,
    sequence: [AtomicU64; COLUMNS],
    frames: [AtomicU64; COLUMNS],
    rates: [AtomicU32; COLUMNS],
    values: [[AtomicU32; 4]; COLUMNS],
}
impl Default for Slot {
    fn default() -> Self {
        Self {
            id: std::array::from_fn(|_| AtomicU64::new(0)),
            bypassed: AtomicBool::new(false),
            length: AtomicU32::new(0),
            cursor: AtomicU32::new(0),
            sequence: std::array::from_fn(|_| AtomicU64::new(0)),
            frames: std::array::from_fn(|_| AtomicU64::new(0)),
            rates: std::array::from_fn(|_| AtomicU32::new(0)),
            values: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU32::new(0))),
        }
    }
}
impl Slot {
    fn push(&self, id: u128, bypassed: bool, block: Block, sequence: u64) {
        let column = block.levels;
        let old = (self.id[0].load(Ordering::Relaxed) as u128) << 64
            | self.id[1].load(Ordering::Relaxed) as u128;
        if old != id {
            self.length.store(0, Ordering::Relaxed);
            self.cursor.store(0, Ordering::Relaxed);
        }
        self.id[0].store((id >> 64) as u64, Ordering::Relaxed);
        self.id[1].store(id as u64, Ordering::Relaxed);
        self.bypassed.store(bypassed, Ordering::Relaxed);
        let at = self.cursor.load(Ordering::Relaxed) as usize;
        self.sequence[at].store(sequence, Ordering::Relaxed);
        self.frames[at].store(block.frames, Ordering::Relaxed);
        self.rates[at].store(block.sample_rate, Ordering::Relaxed);
        for (target, value) in
            self.values[at]
                .iter()
                .zip([column.input, column.output, column.level, column.gain])
        {
            target.store(value.to_bits(), Ordering::Relaxed);
        }
        self.cursor
            .store(((at + 1) % COLUMNS) as u32, Ordering::Relaxed);
        self.length.store(
            (self.length.load(Ordering::Relaxed) + 1).min(COLUMNS as u32),
            Ordering::Relaxed,
        );
    }
    fn read(&self) -> History {
        let id = (self.id[0].load(Ordering::Relaxed) as u128) << 64
            | self.id[1].load(Ordering::Relaxed) as u128;
        let length = self.length.load(Ordering::Relaxed).min(COLUMNS as u32) as usize;
        let cursor = self.cursor.load(Ordering::Relaxed) as usize;
        let columns = (0..length)
            .map(|i| {
                let at = (cursor + COLUMNS - length + i) % COLUMNS;
                let [input, output, level, gain] = std::array::from_fn(|k| {
                    f32::from_bits(self.values[at][k].load(Ordering::Relaxed))
                });
                Observation {
                    sequence: self.sequence[at].load(Ordering::Relaxed),
                    frames: self.frames[at].load(Ordering::Relaxed),
                    sample_rate: self.rates[at].load(Ordering::Relaxed),
                    levels: Column {
                        input,
                        output,
                        level,
                        gain,
                    },
                }
            })
            .collect();
        History {
            instance_id: uuid::Uuid::from_u128(id).to_string(),
            bypassed: self.bypassed.load(Ordering::Relaxed),
            columns,
        }
    }
}
pub(super) struct Bank {
    slots: Box<[Slot]>,
    length: AtomicU32,
    truncated: AtomicBool,
}
impl Default for Bank {
    fn default() -> Self {
        Self {
            // Heap construction avoids large by-value temporaries on native Windows stacks.
            slots: (0..STREAMS)
                .map(|_| Slot::default())
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            length: AtomicU32::new(0),
            truncated: AtomicBool::new(false),
        }
    }
}
impl Bank {
    pub(super) fn publish(&self, plan: Option<&Plan>, sequence: u64) {
        let mut n = 0;
        let mut truncated = false;
        let mut visit = |id, bypassed, block: Block| {
            let column = block.levels;
            if ![column.input, column.output, column.level, column.gain]
                .into_iter()
                .all(f32::is_finite)
            {
                return;
            }
            if n < STREAMS {
                self.slots[n].push(id, bypassed, block, sequence);
                n += 1;
            } else {
                truncated = true;
            }
        };
        if let Some(plan) = plan {
            if let Some(voice) = &plan.voice {
                voice.chain.visit_meters(&mut visit);
            }
            if let Some(stream) = &plan.mix_voice {
                stream.visit_meters(&mut visit);
            }
        }
        self.length.store(n as u32, Ordering::Relaxed);
        self.truncated.store(truncated, Ordering::Relaxed);
    }
    pub(super) fn read(&self) -> (Vec<History>, bool) {
        let length = self.length.load(Ordering::Relaxed).min(STREAMS as u32) as usize;
        (
            (0..length).map(|i| self.slots[i].read()).collect(),
            self.truncated.load(Ordering::Relaxed),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_meter_bank_constructs_on_a_small_native_stack() {
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(|| {
                let bank = Bank::default();
                assert_eq!(bank.slots.len(), STREAMS);
                assert!(bank.read().0.is_empty());
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
