use crate::Tools::{make_schema, Tool, ToolParams, ToolResult, ToolSchema};
use std::sync::Mutex;

/// 全局计划存储（跨调用持久化）
static PLANS: std::sync::LazyLock<Mutex<Vec<Plan>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

const MAX_STEPS: usize = 200;

#[derive(Debug, Clone)]
struct Plan {
    id: String,
    name: String,
    phases: Vec<Phase>,
}

#[derive(Debug, Clone)]
struct Phase {
    id: String,
    title: String,
    steps: Vec<Step>,
}

#[derive(Debug, Clone)]
struct Step {
    id: String,
    content: String,
    status: StepStatus,
    position: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum StepStatus {
    Pending,
    InProgress,
    Done,
    Skipped,
}

impl StepStatus {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "in_progress" => Some(Self::InProgress),
            "done" => Some(Self::Done),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            Self::Pending => "○",
            Self::InProgress => "◉",
            Self::Done => "●",
            Self::Skipped => "◌",
        }
    }
}

pub struct TodoTool;

impl Tool for TodoTool {
    fn name(&self) -> &'static str {
        "Todo"
    }

    fn description(&self) -> &'static str {
        "制定计划(最多200步) / Plan maker (max 200 steps).  \
         action=new_plan|add_phase|add_step|insert|update|delete_step|list|summary  \
         new_plan: name=计划名  \
         add_phase: plan_id, title=阶段名  \
         add_step: plan_id, phase_id, content=步骤内容  \
         insert: plan_id, phase_id, before_step_id, content (插队)  \
         update: plan_id, step_id, content?, status?(pending|in_progress|done|skipped)  \
         delete_step: plan_id, step_id  \
         list: plan_id  \
         summary: (列出所有计划概览)"
    }

    fn schema(&self) -> ToolSchema {
        make_schema(
            "Todo",
            "Create and manage plans with phases and steps (max 200) / 制定分阶段计划，管理步骤",
            &[
                ("action", "string", "Action: new_plan, add_phase, add_step, insert, update, delete_step, list, summary / 操作", true),
                ("name", "string", "Plan name (for new_plan) / 计划名", false),
                ("plan_id", "string", "Plan ID / 计划ID", false),
                ("phase_id", "string", "Phase ID / 阶段ID", false),
                ("step_id", "string", "Step ID (for update/delete) / 步骤ID", false),
                ("title", "string", "Phase title (for add_phase) / 阶段标题", false),
                ("content", "string", "Step content / 步骤内容", false),
                ("status", "string", "Step status: pending, in_progress, done, skipped / 状态", false),
                ("before_step_id", "string", "Insert before this step (for insert) / 插入位置", false),
            ],
        )
    }

    fn execute(&self, params: &ToolParams) -> ToolResult {
        let action = params
            .get("action")
            .map(|s| s.as_str())
            .unwrap_or("summary");

        match action {
            "new_plan" => new_plan(params),
            "add_phase" => add_phase(params),
            "add_step" => add_step(params),
            "insert" => insert_step(params),
            "update" => update_step(params),
            "delete_step" => delete_step(params),
            "list" => list_plan(params),
            "summary" => summary(),
            _ => ToolResult::err(format!("未知操作 / Unknown action: {action}")),
        }
    }
}

fn new_plan(params: &ToolParams) -> ToolResult {
    let name = match params.get("name") {
        Some(n) => n.clone(),
        None => return ToolResult::err("缺少参数 / Missing param: name"),
    };

    let mut plans = PLANS.lock().unwrap();
    let id = format!("p{}", plans.len() + 1);

    plans.push(Plan {
        id: id.clone(),
        name,
        phases: Vec::new(),
    });

    ToolResult::ok(format!(
        "计划已创建 / Plan created: [{id}] (共{}个计划)",
        plans.len()
    ))
}

fn add_phase(params: &ToolParams) -> ToolResult {
    let plan_id = match params.get("plan_id") {
        Some(p) => p.clone(),
        None => return ToolResult::err("缺少参数 / Missing param: plan_id"),
    };
    let title = match params.get("title") {
        Some(t) => t.clone(),
        None => return ToolResult::err("缺少参数 / Missing param: title"),
    };

    let mut plans = PLANS.lock().unwrap();
    let plan = match get_plan_mut(&mut plans, &plan_id) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let pid = format!("ph{}", plan.phases.len() + 1);
    plan.phases.push(Phase {
        id: pid.clone(),
        title,
        steps: Vec::new(),
    });

    ToolResult::ok(format!(
        "阶段已添加 / Phase added: [{pid}] in [{plan_id}] {}",
        plan.name
    ))
}

fn add_step(params: &ToolParams) -> ToolResult {
    let (plan_id, phase_id, content) = match take3(params, "plan_id", "phase_id", "content") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let mut plans = PLANS.lock().unwrap();

    let total_steps: usize = plans
        .iter()
        .map(|p| p.phases.iter().map(|ph| ph.steps.len()).sum::<usize>())
        .sum();
    if total_steps >= MAX_STEPS {
        return ToolResult::err(format!(
            "计划步数已满({MAX_STEPS}) / Plan full ({MAX_STEPS} steps)"
        ));
    }

    let plan = match get_plan_mut(&mut plans, &plan_id) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let phase = match get_phase_mut(plan, &phase_id) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let sid = format!("s{}", phase.steps.len() + 1);
    let pos = phase.steps.len() + 1;

    phase.steps.push(Step {
        id: sid.clone(),
        content,
        status: StepStatus::Pending,
        position: pos,
    });

    ToolResult::ok(format!(
        "步骤已添加 / Step added: [{sid}] #{pos} in [{phase_id}] {title}",
        title = phase.title
    ))
}

fn insert_step(params: &ToolParams) -> ToolResult {
    let plan_id = match params.get("plan_id") {
        Some(p) => p.clone(),
        None => return ToolResult::err("缺少参数 / Missing param: plan_id"),
    };
    let phase_id = match params.get("phase_id") {
        Some(p) => p.clone(),
        None => return ToolResult::err("缺少参数 / Missing param: phase_id"),
    };
    let content = match params.get("content") {
        Some(c) => c.clone(),
        None => return ToolResult::err("缺少参数 / Missing param: content"),
    };
    let before_id = match params.get("before_step_id") {
        Some(b) => b.clone(),
        None => return ToolResult::err("缺少参数 / Missing param: before_step_id"),
    };

    let mut plans = PLANS.lock().unwrap();

    let total_steps: usize = plans
        .iter()
        .map(|p| p.phases.iter().map(|ph| ph.steps.len()).sum::<usize>())
        .sum();
    if total_steps >= MAX_STEPS {
        return ToolResult::err(format!("计划步数已满({MAX_STEPS}) / Plan full"));
    }

    let plan = match get_plan_mut(&mut plans, &plan_id) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let phase = match get_phase_mut(plan, &phase_id) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let before_idx = match phase.steps.iter().position(|s| s.id == before_id) {
        Some(i) => i,
        None => return ToolResult::err(format!("步骤不存在 / Step not found: {before_id}")),
    };

    let insert_pos = phase.steps[before_idx].position;
    for s in phase.steps.iter_mut() {
        if s.position >= insert_pos {
            s.position += 1;
        }
    }

    let sid = generate_step_id(phase);
    phase.steps.insert(
        before_idx,
        Step {
            id: sid.clone(),
            content: content.clone(),
            status: StepStatus::Pending,
            position: insert_pos,
        },
    );

    ToolResult::ok(format!(
        "插队成功 / Inserted: [{sid}] #{insert_pos} before [{before_id}] in {title}",
        title = phase.title
    ))
}

fn update_step(params: &ToolParams) -> ToolResult {
    let plan_id = match params.get("plan_id") {
        Some(p) => p.clone(),
        None => return ToolResult::err("缺少参数 / Missing param: plan_id"),
    };
    let step_id = match params.get("step_id") {
        Some(s) => s.clone(),
        None => return ToolResult::err("缺少参数 / Missing param: step_id"),
    };

    let mut plans = PLANS.lock().unwrap();
    let plan = match get_plan_mut(&mut plans, &plan_id) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let (phase_title, step) = match find_step_mut(plan, &step_id) {
        Ok(v) => v,
        Err(e) => return e,
    };

    let mut changes = Vec::new();
    if let Some(content) = params.get("content") {
        step.content = content.clone();
        changes.push("内容/content");
    }
    if let Some(status_str) = params.get("status") {
        match StepStatus::from_str(status_str) {
            Some(s) => {
                step.status = s;
                changes.push("状态/status");
            }
            None => return ToolResult::err(format!("无效状态 / Invalid status: {status_str}")),
        }
    }

    if changes.is_empty() {
        return ToolResult::err("未提供修改内容 / No changes (content or status)");
    }

    ToolResult::ok(format!(
        "已更新 / Updated [{step_id}] in {phase_title}: {changes} → {icon} {content}",
        changes = changes.join("+"),
        icon = step.status.icon(),
        content = step.content,
    ))
}

fn delete_step(params: &ToolParams) -> ToolResult {
    let plan_id = match params.get("plan_id") {
        Some(p) => p.clone(),
        None => return ToolResult::err("缺少参数 / Missing param: plan_id"),
    };
    let step_id = match params.get("step_id") {
        Some(s) => s.clone(),
        None => return ToolResult::err("缺少参数 / Missing param: step_id"),
    };

    let mut plans = PLANS.lock().unwrap();
    let plan = match get_plan_mut(&mut plans, &plan_id) {
        Ok(p) => p,
        Err(e) => return e,
    };

    for phase in plan.phases.iter_mut() {
        if let Some(idx) = phase.steps.iter().position(|s| s.id == step_id) {
            let removed = phase.steps.remove(idx);
            reorder_steps(&mut phase.steps);
            return ToolResult::ok(format!(
                "已删除 / Deleted [{step_id}] from {title}: {content}",
                title = phase.title,
                content = removed.content,
            ));
        }
    }

    ToolResult::err(format!("步骤不存在 / Step not found: {step_id}"))
}

fn list_plan(params: &ToolParams) -> ToolResult {
    let plan_id = match params.get("plan_id") {
        Some(p) => p.clone(),
        None => return ToolResult::err("缺少参数 / Missing param: plan_id"),
    };

    let plans = PLANS.lock().unwrap();
    let plan = match plans.iter().find(|p| p.id == plan_id) {
        Some(p) => p,
        None => return ToolResult::err(format!("计划不存在 / Plan not found: {plan_id}")),
    };

    let mut lines = vec![format!("计划 / Plan [{plan_id}]: {}", plan.name)];

    for phase in &plan.phases {
        lines.push(format!(
            "  ── {title} [{pid}] ──",
            title = phase.title,
            pid = phase.id
        ));

        if phase.steps.is_empty() {
            lines.push("    (空 / empty)".to_string());
        } else {
            for step in &phase.steps {
                lines.push(format!(
                    "    {icon} #{pos:>2} [{sid}] {content}",
                    icon = step.status.icon(),
                    pos = step.position,
                    sid = step.id,
                    content = step.content,
                ));
            }
        }
    }

    let total: usize = plan.phases.iter().map(|ph| ph.steps.len()).sum();
    ToolResult::ok_with_data(
        format!("计划详情 / Plan detail: {total} 步 total"),
        lines.join("\n"),
    )
}

fn summary() -> ToolResult {
    let plans = PLANS.lock().unwrap();

    if plans.is_empty() {
        return ToolResult::ok("暂无计划 / No plans yet. Use new_plan to create one.");
    }

    let mut lines = Vec::new();
    for plan in plans.iter() {
        let total: usize = plan.phases.iter().map(|ph| ph.steps.len()).sum();
        let done = plan
            .phases
            .iter()
            .flat_map(|ph| ph.steps.iter())
            .filter(|s| s.status == StepStatus::Done)
            .count();
        let in_prog = plan
            .phases
            .iter()
            .flat_map(|ph| ph.steps.iter())
            .filter(|s| s.status == StepStatus::InProgress)
            .count();

        lines.push(format!(
            "  [{id}] {name} — {phases}阶段, {total}步, ●{done} ◉{in_prog}",
            id = plan.id,
            name = plan.name,
            phases = plan.phases.len(),
        ));
    }

    ToolResult::ok_with_data(
        format!("共 {} 个计划 / {} plan(s)", plans.len(), plans.len()),
        lines.join("\n"),
    )
}

fn take3(
    params: &ToolParams,
    k1: &str,
    k2: &str,
    k3: &str,
) -> Result<(String, String, String), ToolResult> {
    let v1 = params
        .get(k1)
        .cloned()
        .ok_or_else(|| ToolResult::err(format!("缺少参数 / Missing param: {k1}")))?;
    let v2 = params
        .get(k2)
        .cloned()
        .ok_or_else(|| ToolResult::err(format!("缺少参数 / Missing param: {k2}")))?;
    let v3 = params
        .get(k3)
        .cloned()
        .ok_or_else(|| ToolResult::err(format!("缺少参数 / Missing param: {k3}")))?;
    Ok((v1, v2, v3))
}

fn get_plan_mut<'a>(plans: &'a mut [Plan], id: &str) -> Result<&'a mut Plan, ToolResult> {
    plans
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| ToolResult::err(format!("计划不存在 / Plan not found: {id}")))
}

fn get_phase_mut<'a>(plan: &'a mut Plan, id: &str) -> Result<&'a mut Phase, ToolResult> {
    plan.phases
        .iter_mut()
        .find(|ph| ph.id == id)
        .ok_or_else(|| ToolResult::err(format!("阶段不存在 / Phase not found: {id}")))
}

fn find_step_mut<'a>(
    plan: &'a mut Plan,
    step_id: &str,
) -> Result<(String, &'a mut Step), ToolResult> {
    for phase in plan.phases.iter_mut() {
        if let Some(step) = phase.steps.iter_mut().find(|s| s.id == step_id) {
            return Ok((phase.title.clone(), step));
        }
    }
    Err(ToolResult::err(format!(
        "步骤不存在 / Step not found: {step_id}"
    )))
}

fn generate_step_id(phase: &Phase) -> String {
    let max_n = phase
        .steps
        .iter()
        .filter_map(|s| s.id.strip_prefix('s').and_then(|n| n.parse::<usize>().ok()))
        .max()
        .unwrap_or(0);
    format!("s{}", max_n + 1)
}

fn reorder_steps(steps: &mut [Step]) {
    steps.sort_by_key(|s| s.position);
    for (i, s) in steps.iter_mut().enumerate() {
        s.position = i + 1;
    }
}
