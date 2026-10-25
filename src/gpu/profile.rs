use super::*;
use crate::RenderProfile;

// One submission is in flight. Reuse three queries only after its event completes.
pub(super) struct Profile {
    disjoint: ID3D11Query,
    start: ID3D11Query,
    end: ID3D11Query,
    pub(super) stats: RenderProfile,
}

impl Profile {
    pub(super) fn new(device: &ID3D11Device) -> Result<Self, Error> {
        let query = |kind| -> Result<ID3D11Query, Error> {
            let mut output = None;
            // SAFETY: valid query description and live output; COM result is owned.
            unsafe {
                device.CreateQuery(
                    &D3D11_QUERY_DESC {
                        Query: kind,
                        MiscFlags: 0,
                    },
                    Some(&mut output),
                )?;
            }
            Ok(output.expect("query output"))
        };
        Ok(Self {
            disjoint: query(D3D11_QUERY_TIMESTAMP_DISJOINT)?,
            start: query(D3D11_QUERY_TIMESTAMP)?,
            end: query(D3D11_QUERY_TIMESTAMP)?,
            stats: RenderProfile::default(),
        })
    }

    pub(super) fn begin(&self, context: &ID3D11DeviceContext) {
        // SAFETY: exclusive worker context; previous queries completed before reuse.
        unsafe {
            context.Begin(&self.disjoint);
            context.End(&self.start);
        }
    }

    pub(super) fn end(&self, context: &ID3D11DeviceContext) {
        // SAFETY: paired with begin on the same exclusive context and live queries.
        unsafe {
            context.End(&self.end);
            context.End(&self.disjoint);
        }
    }

    pub(super) fn collect(&mut self, context: &ID3D11DeviceContext) -> Result<(), Error> {
        let disjoint: Option<D3D11_QUERY_DATA_TIMESTAMP_DISJOINT> = read(context, &self.disjoint)?;
        let start: Option<u64> = read(context, &self.start)?;
        let end: Option<u64> = read(context, &self.end)?;
        let duration = disjoint.zip(start).zip(end).and_then(|((d, start), end)| {
            duration_ns(start, end, d.Frequency, d.Disjoint.as_bool())
        });
        if let Some(duration) = duration {
            self.stats.gpu_frames += 1;
            self.stats.gpu_draw_ns += duration;
        } else {
            self.stats.invalid_gpu_frames += 1;
        }
        Ok(())
    }
}

fn duration_ns(start: u64, end: u64, frequency: u64, disjoint: bool) -> Option<u64> {
    if disjoint || frequency == 0 {
        return None;
    }
    u64::try_from(u128::from(end.checked_sub(start)?) * 1_000_000_000 / u128::from(frequency)).ok()
}

fn read<T: Default>(
    context: &ID3D11DeviceContext,
    query: &ID3D11Query,
) -> Result<Option<T>, Error> {
    let mut value = T::default();
    // SAFETY: this module pairs u64 with timestamp and the D3D11 disjoint struct
    // with disjoint queries. Output is correctly sized/aligned and remains live.
    let result = unsafe {
        (context.vtable().GetData)(
            context.as_raw(),
            query.as_raw(),
            (&mut value as *mut T).cast(),
            size_of::<T>() as u32,
            D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32,
        )
    };
    match result {
        S_OK => Ok(Some(value)),
        S_FALSE => Ok(None),
        error => Err(Error::Graphics(windows::core::Error::from_hresult(error))),
    }
}

#[cfg(test)]
mod tests {
    use super::duration_ns;
    #[test]
    fn invalid_timestamps_are_excluded_instead_of_reported_as_zero() {
        assert_eq!(duration_ns(10, 20, 1000, false), Some(10_000_000));
        assert_eq!(duration_ns(10, 20, 1000, true), None);
        assert_eq!(duration_ns(10, 20, 0, false), None);
        assert_eq!(duration_ns(20, 10, 1000, false), None);
        assert_eq!(duration_ns(0, u64::MAX, 1, false), None);
    }
}
