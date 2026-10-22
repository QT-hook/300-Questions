use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Sync,
    Independent,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub online: bool,
    pub ready: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub member: String,
    pub position: usize,
    pub choice: usize,
    pub submission: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub code: String,
    pub fingerprint: String,
    pub term: u64,
    pub host: String,
    pub members: Vec<Member>,
    pub mode: Mode,
    pub indices: Vec<usize>,
    pub answers: Vec<Answer>,
    pub started: bool,
    pub cursor: usize,
    pub reveal_until: Option<u64>,
    pub notice: Option<String>,
}

fn validate_member(member: &Member) -> Result<(), String> {
    if member.id.trim().is_empty() || !(1..=9).contains(&member.name.trim().chars().count()) {
        return Err("成员 ID 不能为空，昵称须为 1–9 个字符".into());
    }
    Ok(())
}

impl Room {
    pub fn new(
        name: String,
        code: String,
        member: Member,
        fingerprint: String,
    ) -> Result<Self, String> {
        let room = Self {
            id: format!("{:032x}", rand::random::<u128>()),
            name,
            code,
            fingerprint,
            term: 1,
            host: member.id.clone(),
            members: vec![member],
            mode: Mode::Independent,
            indices: Vec::new(),
            answers: Vec::new(),
            started: false,
            cursor: 0,
            reveal_until: None,
            notice: None,
        };
        room.validate(0)?;
        Ok(room)
    }

    pub fn add_member(&mut self, member: Member) -> Result<(), String> {
        validate_member(&member)?;
        if let Some(existing) = self.members.iter_mut().find(|m| m.id == member.id) {
            *existing = member;
            return Ok(());
        }
        if self.started {
            return Err("房间已开始，仅原成员可重连".into());
        }
        if self.members.len() >= 32 {
            return Err("房间最多允许 32 人".into());
        }
        self.members.push(member);
        Ok(())
    }

    pub fn start(
        &mut self,
        indices: Vec<usize>,
        mode: Mode,
        bank_len: usize,
    ) -> Result<(), String> {
        self.validate(bank_len)?;
        if self.started {
            return Err("房间已开始".into());
        }
        if indices.is_empty()
            || indices.iter().any(|&i| i >= bank_len)
            || indices.iter().collect::<HashSet<_>>().len() != indices.len()
        {
            return Err("题目索引为空、重复或越界".into());
        }
        if self
            .members
            .iter()
            .any(|m| m.online && m.id != self.host && !m.ready)
        {
            return Err("在线成员尚未全部准备".into());
        }
        self.indices = indices;
        self.mode = mode;
        self.started = true;
        self.cursor = 0;
        self.answers.clear();
        self.reveal_until = None;
        Ok(())
    }

    /// Returns true only when a new immutable ledger entry was recorded.
    pub fn submit(&mut self, answer: Answer, bank_len: usize) -> Result<bool, String> {
        self.validate(bank_len)?;
        self.validate_answer(&answer)?;
        if let Some(existing) = self
            .answers
            .iter()
            .find(|a| a.member == answer.member && a.position == answer.position)
        {
            if existing != &answer {
                self.notice = Some("答案冲突：保留首次提交".into());
            }
            return Ok(false);
        }
        if answer.position != self.position(&answer.member) {
            return Err("只能提交当前题目".into());
        }
        if self.mode == Mode::Sync && self.reveal_until.is_some() {
            return Err("当前题目正在揭晓".into());
        }
        self.answers.push(answer);
        Ok(true)
    }

    pub fn tick(&mut self, now_ms: u64) {
        if !self.started || self.mode != Mode::Sync || self.cursor >= self.indices.len() {
            return;
        }
        if let Some(deadline) = self.reveal_until {
            if now_ms >= deadline {
                self.cursor += 1;
                self.reveal_until = None;
            }
            return;
        }
        let mut online = self.members.iter().filter(|m| m.online).peekable();
        if online.peek().is_some() && online.all(|m| self.has_answer(&m.id, self.cursor)) {
            self.reveal_until = Some(now_ms.saturating_add(2_000));
        }
    }

    fn has_answer(&self, id: &str, position: usize) -> bool {
        self.answers
            .iter()
            .any(|a| a.member == id && a.position == position)
    }

    pub fn position(&self, id: &str) -> usize {
        if self.mode == Mode::Independent {
            (0..self.indices.len())
                .find(|&p| !self.has_answer(id, p))
                .unwrap_or(self.indices.len())
        } else {
            self.cursor
        }
    }

    pub fn completed(&self, id: &str) -> bool {
        self.started
            && self.members.iter().any(|m| m.id == id)
            && (0..self.indices.len()).all(|p| self.has_answer(id, p))
    }

    pub fn score(&self, id: &str, correct: &[usize]) -> usize {
        (0..self.indices.len())
            .filter(|&p| {
                let Some(&expected) = self.indices.get(p).and_then(|&i| correct.get(i)) else {
                    return false;
                };
                self.answers
                    .iter()
                    .find(|a| a.member == id && a.position == p)
                    .is_some_and(|a| a.choice == expected)
            })
            .count()
    }

    fn validate_answer(&self, answer: &Answer) -> Result<(), String> {
        if !self.started
            || answer.position >= self.indices.len()
            || answer.choice >= 4
            || answer.submission.trim().is_empty()
            || !self.members.iter().any(|m| m.id == answer.member)
        {
            return Err("提交包含未知成员、无效题号、选项或提交 ID".into());
        }
        Ok(())
    }

    /// Checks untrusted snapshots against the local question-bank bounds.
    pub fn validate(&self, bank_len: usize) -> Result<(), String> {
        if self.id.trim().is_empty()
            || !(2..=6).contains(&self.name.trim().chars().count())
            || self.code.len() != 4
            || !self.code.bytes().all(|b| b.is_ascii_digit())
            || self.members.is_empty()
            || self.members.len() > 32
        {
            return Err("房间信息无效：名称须为 2–6 字符、口令须为 4 字符、人数须为 1–32".into());
        }
        let mut ids = HashSet::new();
        for member in &self.members {
            validate_member(member)?;
            if !ids.insert(&member.id) {
                return Err("成员 ID 重复".into());
            }
        }
        if !ids.contains(&self.host) {
            return Err("房主不在成员列表中".into());
        }
        if self.indices.iter().any(|&i| i >= bank_len)
            || self.indices.iter().collect::<HashSet<_>>().len() != self.indices.len()
            || self.cursor > self.indices.len()
            || (self.started && self.indices.is_empty())
            || (!self.started && (!self.answers.is_empty() || self.cursor != 0))
            || (self.reveal_until.is_some()
                && (!self.started || self.mode != Mode::Sync || self.cursor == self.indices.len()))
        {
            return Err("题目索引或进度无效".into());
        }
        let mut keys = HashSet::new();
        for answer in &self.answers {
            self.validate_answer(answer)?;
            if !keys.insert((&answer.member, answer.position)) {
                return Err("答案记录重复".into());
            }
        }
        Ok(())
    }

    pub fn merge(&mut self, other: &Self) -> Result<(), String> {
        self.validate(usize::MAX)?;
        other.validate(usize::MAX)?;
        if self.id != other.id || self.code != other.code || self.fingerprint != other.fingerprint {
            return Err("无法合并不同房间或不同题库".into());
        }
        if self.started
            && other.started
            && (self.indices != other.indices || self.mode != other.mode)
        {
            return Err("无法合并不同答题配置".into());
        }
        let incoming_wins =
            other.term > self.term || (other.term == self.term && other.host < self.host);
        let (winner, loser) = if incoming_wins {
            (other, &*self)
        } else {
            (&*self, other)
        };
        let mut merged = winner.clone();
        for member in &loser.members {
            if !merged.members.iter().any(|m| m.id == member.id) {
                merged.members.push(member.clone());
            }
        }
        if self.term == other.term && self.host == other.host {
            // Readiness is monotonic within a coordinator term; stale snapshots
            // must not undo a guest's ready announcement.
            for member in &mut merged.members {
                if other.members.iter().any(|m| m.id == member.id && m.ready)
                    || self.members.iter().any(|m| m.id == member.id && m.ready)
                {
                    member.ready = true;
                }
            }
            if self.cursor == other.cursor {
                merged.reveal_until = self.reveal_until.max(other.reveal_until);
            }
        }
        if self.started || other.started {
            let active = if self.started { &*self } else { other };
            merged.started = true;
            merged.indices.clone_from(&active.indices);
            merged.mode = active.mode;
        }
        // Local ledger entries are immutable even when remote metadata wins.
        merged.answers.clone_from(&self.answers);
        for answer in &other.answers {
            if let Some(existing) = merged
                .answers
                .iter()
                .find(|a| a.member == answer.member && a.position == answer.position)
            {
                if existing != answer {
                    merged.notice = Some("答案冲突：保留首次提交".into());
                }
            } else {
                merged.answers.push(answer.clone());
            }
        }
        merged.cursor = self.cursor.max(other.cursor);
        if self.cursor != other.cursor {
            merged.reveal_until = None;
            merged.notice = Some("进度分歧：已保留较大进度，答案记录未丢弃".into());
        }
        merged.validate(usize::MAX)?;
        *self = merged;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str) -> Member {
        Member {
            id: id.into(),
            name: id.into(),
            endpoint: "127.0.0.1:1".into(),
            online: true,
            ready: true,
        }
    }
    fn room() -> Room {
        Room::new(
            "测试房".into(),
            "1234".into(),
            member("host"),
            "bank".into(),
        )
        .unwrap()
    }
    fn answer(id: &str, position: usize) -> Answer {
        Answer {
            member: id.into(),
            position,
            choice: 1,
            submission: format!("{id}-{position}"),
        }
    }

    #[test]
    fn creation_checks_unicode_name_lengths() {
        assert!(Room::new("单".into(), "1234".into(), member("h"), "b".into()).is_err());
        assert!(
            Room::new(
                "中文房间名称".into(),
                "1234".into(),
                member("h"),
                "b".into()
            )
            .is_ok()
        );
    }
    #[test]
    fn creation_rejects_invalid_code_and_member() {
        assert!(Room::new("房间".into(), "123".into(), member("h"), "b".into()).is_err());
        assert!(Room::new("房间".into(), "1234".into(), member(""), "b".into()).is_err());
    }
    #[test]
    fn membership_limits_new_members_but_allows_reconnection() {
        let mut r = room();
        for i in 0..31 {
            r.add_member(member(&i.to_string())).unwrap();
        }
        assert!(r.add_member(member("extra")).is_err());
        let mut host = member("host");
        host.online = false;
        r.add_member(host).unwrap();
        assert_eq!(r.members.len(), 32);
        assert!(!r.members[0].online);
    }
    #[test]
    fn waiting_configuration_can_be_validated_and_started() {
        let mut r = room();
        r.indices = vec![1, 0];
        r.validate(2).unwrap();
        r.start(r.indices.clone(), Mode::Sync, 2).unwrap();
        assert!(r.started);
    }

    #[test]
    fn start_requires_only_online_guests_to_be_ready() {
        let mut r = room();
        r.members[0].ready = false;
        let mut guest = member("guest");
        guest.ready = false;
        r.add_member(guest).unwrap();
        assert!(r.start(vec![0], Mode::Sync, 1).is_err());
        r.members[1].online = false;
        r.start(vec![0], Mode::Sync, 1).unwrap();
    }
    #[test]
    fn start_rejects_empty_duplicate_and_out_of_bounds_indices_atomically() {
        let mut r = room();
        let before = r.clone();
        for indices in [vec![], vec![0, 0], vec![2]] {
            assert!(r.start(indices, Mode::Sync, 2).is_err());
            assert_eq!(r, before);
        }
    }
    #[test]
    fn first_submission_is_immutable_and_retries_are_idempotent() {
        let mut r = room();
        r.start(vec![0], Mode::Sync, 1).unwrap();
        let a = answer("host", 0);
        assert_eq!(r.submit(a.clone(), 1), Ok(true));
        assert_eq!(r.submit(a.clone(), 1), Ok(false));
        let mut changed = a.clone();
        changed.choice = 2;
        assert_eq!(r.submit(changed, 1), Ok(false));
        assert_eq!(r.answers, vec![a]);
        assert!(r.notice.is_some());
    }
    #[test]
    fn submit_rejects_unknown_member_invalid_choice_and_future_position() {
        let mut r = room();
        r.start(vec![0, 1], Mode::Sync, 2).unwrap();
        assert!(r.submit(answer("unknown", 0), 2).is_err());
        assert!(r.submit(answer("host", 1), 2).is_err());
        let mut a = answer("host", 0);
        a.choice = 4;
        assert!(r.submit(a, 2).is_err());
        assert!(r.answers.is_empty());
    }
    #[test]
    fn synchronous_reveal_waits_two_seconds_and_ignores_offline_members() {
        let mut r = room();
        let mut offline = member("away");
        offline.online = false;
        r.add_member(offline).unwrap();
        r.start(vec![0], Mode::Sync, 1).unwrap();
        r.tick(10);
        assert_eq!(r.reveal_until, None);
        r.submit(answer("host", 0), 1).unwrap();
        r.tick(100);
        assert_eq!(r.reveal_until, Some(2100));
        r.tick(2099);
        assert_eq!(r.cursor, 0);
        r.tick(2100);
        assert_eq!((r.cursor, r.reveal_until), (1, None));
        r.tick(u64::MAX);
        assert_eq!(r.cursor, 1);
    }
    #[test]
    fn no_online_members_does_not_advance() {
        let mut r = room();
        r.start(vec![0], Mode::Sync, 1).unwrap();
        r.members[0].online = false;
        r.tick(0);
        assert_eq!(r.reveal_until, None);
    }
    #[test]
    fn independent_progress_and_scores_use_shuffled_bank_indices() {
        let mut r = room();
        r.start(vec![2, 0], Mode::Independent, 3).unwrap();
        r.submit(answer("host", 0), 3).unwrap();
        assert_eq!(r.position("host"), 1);
        assert!(!r.completed("host"));
        r.submit(answer("host", 1), 3).unwrap();
        r.tick(100);
        assert_eq!(r.cursor, 0);
        assert!(r.completed("host"));
        assert!(!r.completed("unknown"));
        assert_eq!(r.score("host", &[0, 0, 1]), 1);
        assert_eq!(r.score("host", &[]), 0);
    }
    #[test]
    fn independent_position_finds_merged_holes() {
        let mut r = room();
        r.start(vec![0, 1], Mode::Independent, 2).unwrap();
        r.answers.push(answer("host", 1));
        assert_eq!(r.position("host"), 0);
        assert!(!r.completed("host"));
    }
    #[test]
    fn merge_selects_larger_term_then_smaller_host_and_preserves_members() {
        let mut r = room();
        r.add_member(member("aaa")).unwrap();
        let mut other = r.clone();
        other.host = "aaa".into();
        other.name = "新房间".into();
        other.add_member(member("remote")).unwrap();
        r.merge(&other).unwrap();
        assert_eq!(r.host, "aaa");
        assert_eq!(r.members.len(), 3);
        other.term += 1;
        other.host = "host".into();
        r.merge(&other).unwrap();
        assert_eq!(r.host, "host");
    }
    #[test]
    fn merge_unions_answers_preserves_conflicts_and_max_cursor() {
        let mut r = room();
        r.add_member(member("guest")).unwrap();
        r.start(vec![0, 1], Mode::Sync, 2).unwrap();
        r.submit(answer("host", 0), 2).unwrap();
        let mut other = r.clone();
        other.answers[0].choice = 2;
        other.answers.push(answer("guest", 0));
        other.cursor = 1;
        other.term += 1;
        r.merge(&other).unwrap();
        assert_eq!(r.answers[0].choice, 1);
        assert_eq!(r.answers.len(), 2);
        assert_eq!(r.cursor, 1);
        assert!(r.notice.is_some());
    }
    #[test]
    fn merge_same_coordinator_preserves_ready_started_and_reveal() {
        let mut r = room();
        r.members[0].ready = false;
        let stale = r.clone();
        let mut active = r.clone();
        active.members[0].ready = true;
        active.start(vec![0], Mode::Sync, 1).unwrap();
        active.submit(answer("host", 0), 1).unwrap();
        active.tick(100);
        r.merge(&active).unwrap();
        r.merge(&stale).unwrap();
        assert!(r.started);
        assert!(r.members[0].ready);
        assert_eq!(r.reveal_until, Some(2100));
        assert_eq!(r.answers.len(), 1);
    }

    #[test]
    fn merge_rejects_incompatible_identity_without_mutating() {
        let mut r = room();
        let before = r.clone();
        let mut other = r.clone();
        other.fingerprint = "bad".into();
        assert!(r.merge(&other).is_err());
        assert_eq!(r, before);
    }
    #[test]
    fn validate_rejects_untrusted_bounds_and_duplicate_ledger() {
        let mut r = room();
        r.start(vec![1], Mode::Sync, 2).unwrap();
        assert!(r.validate(1).is_err());
        r.answers = vec![answer("host", 0), answer("host", 0)];
        assert!(r.validate(2).is_err());
    }
    #[test]
    fn serde_roundtrip_preserves_all_state() {
        let r = room();
        assert_eq!(
            serde_json::from_str::<Room>(&serde_json::to_string(&r).unwrap()).unwrap(),
            r
        );
    }
}
