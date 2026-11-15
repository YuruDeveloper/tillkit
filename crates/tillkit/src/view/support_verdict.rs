use crate::app::App;
use crate::message::Message;
use crate::support_words;
use crate::view::{parts, support_rows};
use front2::SupportReport;
use iced::Element;
use iced::widget::column;

const UNSUPPORTED: &str = "지원하는 조건과 맞지 않는 항목이 있어서, 이 기기에는 기록하지 않습니다.";
const ROOT_MISSING: &str = "루트 권한이 없어서 이 기기에는 기록할 수 없습니다.";
const BOOT_UNFINISHED: &str =
    "기기가 아직 부팅을 끝내지 않았습니다. 부팅이 끝난 뒤에 다시 확인해 주세요.";
const FORCED: &str = "맞지 않는 항목을 확인하고 강제로 진행하는 중입니다. 파일 대조와 기록 뒤의 확인은 그대로 합니다.";

pub fn unmet_report(app: &App) -> Option<&SupportReport> {
    app.diagnosis
        .as_ref()
        .map(|diagnosis| &diagnosis.report)
        .filter(|report| !report.writes_allowed())
}

pub fn block<'a>(app: &App, report: &SupportReport) -> Element<'a, Message> {
    let palette = app.palette;
    let unmet = support_rows::unmet(report, palette);
    if app.forced {
        return column![parts::caution("FORCED", FORCED, palette), unmet]
            .spacing(10)
            .into();
    }
    let mut block = column![
        parts::fault("NOT SUPPORTED", reason(report), "", palette),
        unmet
    ]
    .spacing(10);
    if support_words::can_be_forced(report) {
        let ask = (!app.is_busy()).then_some(Message::AskForce);
        block = block.push(parts::bracket("CONTINUE ANYWAY", ask, palette));
    }
    block.into()
}

fn reason(report: &SupportReport) -> &'static str {
    if support_words::root_is_missing(report) {
        ROOT_MISSING
    } else if support_words::boot_is_unfinished(report) {
        BOOT_UNFINISHED
    } else {
        UNSUPPORTED
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_paths::DataPaths;
    use crate::diagnosis::tests::{OTHER_BUILD, front2_with_fingerprint};
    use front2::RootAccess;
    use front2::profiles::toss_front2;

    #[test]
    fn the_root_screen_shows_the_unmet_checks_only_for_a_read_device_that_differs() {
        let directory = tempfile::tempdir().expect("tempdir");
        let mut app = App::with_paths(DataPaths::new(directory.path()));
        assert!(unmet_report(&app).is_none());
        app.diagnosis = Some(front2_with_fingerprint(
            toss_front2::VENDOR_BUILD_FINGERPRINT,
            Some(RootAccess::Su),
        ));
        assert!(unmet_report(&app).is_none());
        app.diagnosis = Some(front2_with_fingerprint(OTHER_BUILD, Some(RootAccess::Su)));
        assert!(app.support_stops_writes());
        let report = unmet_report(&app).expect("the unmet checks are shown");
        assert_eq!(reason(report), UNSUPPORTED);
        app.forced = true;
        assert!(!app.support_stops_writes());
        assert!(unmet_report(&app).is_some());
    }

    #[test]
    fn a_device_without_root_is_told_as_a_missing_root() {
        let diagnosis = front2_with_fingerprint(OTHER_BUILD, None);
        assert_eq!(reason(&diagnosis.report), ROOT_MISSING);
        assert!(!support_words::can_be_forced(&diagnosis.report));
    }
}
