//! Prototype links between the mobile screens and the flow starting points.

use super::mobile::Screens;
use super::*;

fn click(dest: NodeId, kind: TransitionKind, ms: u32) -> Reaction {
    Reaction {
        trigger: Trigger::OnClick,
        actions: vec![Action::Navigate { dest, preserve_scroll: false }],
        transition: Some(Transition { kind, easing: Easing::EaseOut, duration_ms: ms }),
    }
}

pub fn wire(s: &mut S, sc: &Screens) {
    let push = TransitionKind::Push(Direction::Left);
    s.n(sc.welcome_cta).prototype.reactions.push(click(sc.home, push, 320));
    s.n(sc.welcome_login).prototype.reactions.push(click(sc.home, TransitionKind::Dissolve, 240));
    s.n(sc.home_balance).prototype.reactions.push(click(sc.spending, push, 320));
    s.n(sc.spending_more).prototype.reactions.push(click(sc.settings, push, 320));
    s.n(sc.spending_back).prototype.reactions.push(Reaction {
        trigger: Trigger::OnClick,
        actions: vec![Action::Back],
        transition: Some(Transition { kind: TransitionKind::Push(Direction::Right), easing: Easing::EaseOut, duration_ms: 280 }),
    });
    s.n(sc.settings_out).prototype.reactions.push(Reaction { trigger: Trigger::OnClick, actions: vec![Action::Back], transition: None });
    let page = s.page(0);
    s.d.flows.push(Flow { name: "Onboarding".into(), page, start: sc.welcome, description: "First launch: welcome, then the home dashboard.".into() });
    s.d.flows.push(Flow { name: "Dashboard".into(), page, start: sc.home, description: "Home, spending detail and settings.".into() });
}
