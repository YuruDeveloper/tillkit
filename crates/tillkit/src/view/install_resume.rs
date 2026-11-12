use front2::install::{InstallPlan, InstallStep, ResumePoint};

const FIRST_INSTALL: &str = "처음 설치입니다. 기기의 내용이 전체 백업과 같습니다.";
const ONLY_RESTART: &str = "기록은 이미 모두 끝나 있습니다. 다시 시작하는 일만 남았습니다.";
const BOOT_LUN: &str = "부팅 위치 바꾸기";

pub fn lines(resume: &ResumePoint, plan: &InstallPlan) -> Vec<String> {
    if resume.first_install {
        return vec![FIRST_INSTALL.to_string()];
    }
    let Some(name) = next_name(resume, plan) else {
        return vec![ONLY_RESTART.to_string()];
    };
    let mut lines = vec![continued(resume.next_index, &name)];
    if let Some(unknown) = &resume.unknown_content {
        lines.push(format!(
            "{unknown}의 내용은 전체 백업과도 이 계획과도 다릅니다. 기록하다가 멈춘 것으로 보고, 이 계획의 이미지로 덮어씁니다."
        ));
    }
    lines
}

fn next_name(resume: &ResumePoint, plan: &InstallPlan) -> Option<String> {
    match plan.steps.get(resume.next_index)? {
        InstallStep::Write(step) => Some(step.name.clone()),
        InstallStep::SetBootLun { .. } => Some(BOOT_LUN.to_string()),
        InstallStep::Restart => None,
    }
}

fn continued(done: usize, name: &str) -> String {
    if done == 0 {
        return format!("멈춘 설치를 이어 합니다. 첫 단계인 {name}부터 다시 기록합니다.");
    }
    format!(
        "멈춘 설치를 이어 합니다. 앞의 {done}단계는 이미 기록되어 있어서, {name}부터 기록합니다."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install_support::InstallFixture;

    fn point(first_install: bool, next_index: usize, unknown: Option<&str>) -> ResumePoint {
        ResumePoint {
            first_install,
            next_index,
            next_name: String::new(),
            unknown_content: unknown.map(str::to_string),
        }
    }

    #[test]
    fn the_lines_tell_where_the_write_starts() {
        let fixture = InstallFixture::standard();
        let plan = InstallPlan::build(&fixture.inputs).expect("plan");
        let writes = plan.write_steps();
        let (second, step) = writes[1];
        let boot_lun = plan
            .steps
            .iter()
            .position(|step| matches!(step, InstallStep::SetBootLun { .. }))
            .expect("the plan changes the boot LUN");
        let restart = plan.steps.len() - 1;

        assert_eq!(lines(&point(true, 0, None), &plan), [FIRST_INSTALL]);

        let continued = lines(&point(false, second, None), &plan);
        assert_eq!(continued.len(), 1);
        assert!(continued[0].contains(&format!("앞의 {second}단계")));
        assert!(continued[0].contains(&format!("{}부터", step.name)));

        let first_name = &writes[0].1.name;
        let unknown = lines(&point(false, 0, Some(first_name)), &plan);
        assert_eq!(unknown.len(), 2);
        assert!(unknown[0].contains("첫 단계"));
        assert!(unknown[1].starts_with(&format!("{first_name}의 내용")));

        let at_boot_lun = lines(&point(false, boot_lun, None), &plan);
        assert!(at_boot_lun[0].contains(BOOT_LUN));
        assert_eq!(lines(&point(false, restart, None), &plan), [ONLY_RESTART]);
    }
}
