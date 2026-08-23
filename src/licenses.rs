use std::io::{self, Write};

pub const PROJECT_LICENSE_TEXT: &str = include_str!("../LICENSE");
pub const THIRD_PARTY_LICENSE_TEXT: &str = include_str!("../THIRD_PARTY_LICENSES.txt");

pub fn print_license_report(component: &str) -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    match write_license_report(&mut output, component) {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        result => result,
    }
}

fn write_license_report(mut output: impl Write, component: &str) -> io::Result<()> {
    writeln!(
        output,
        "{component} project license\n===============================\n"
    )?;
    output.write_all(PROJECT_LICENSE_TEXT.as_bytes())?;
    writeln!(
        output,
        "\nThird-party dependency licenses\n================================\n"
    )?;
    output.write_all(THIRD_PARTY_LICENSE_TEXT.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BrokenPipe;

    impl Write for BrokenPipe {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "consumer closed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn report_contains_both_license_sections() {
        let mut output = Vec::new();
        write_license_report(&mut output, "Test Component").unwrap();
        let report = String::from_utf8(output).unwrap();
        assert!(report.starts_with("Test Component project license\n"));
        assert!(report.contains(PROJECT_LICENSE_TEXT));
        assert!(report.contains("Third-party dependency licenses"));
        assert!(report.ends_with(THIRD_PARTY_LICENSE_TEXT));
    }

    #[test]
    fn writer_reports_a_closed_consumer_without_panicking() {
        let error = write_license_report(BrokenPipe, "Test Component").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    }
}
