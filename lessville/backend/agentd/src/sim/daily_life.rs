//! Scenario parameters, not calibrated human physiology. One tick is one minute.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Life {
    pub hunger: f64,
    pub fatigue: f64,
    pub social_need: f64,
    pub meal: Option<Meal>,
    pub meals_completed: u64,
    pub retry_at: i64,
    pub conversation: Option<Conversation>,
    pub social_after: i64,
    pub practice_minutes: u32,
    pub practice_sessions: u32,
}

impl Default for Life {
    fn default() -> Self {
        Self {
            hunger: 35.0,
            fatigue: 20.0,
            social_need: 15.0,
            meal: None,
            meals_completed: 0,
            retry_at: 0,
            conversation: None,
            social_after: 0,
            practice_minutes: 0,
            practice_sessions: 0,
        }
    }
}

impl Life {
    pub(super) fn minute(&mut self, sleeping: bool) {
        self.hunger = (self.hunger + if sleeping { 0.025 } else { 0.1 }).min(100.0);
        self.fatigue =
            (self.fatigue + if sleeping { -1.0 / 6.0 } else { 1.0 / 15.0 }).clamp(0.0, 100.0);
        self.social_need = (self.social_need + if sleeping { 0.0 } else { 0.05 }).min(100.0);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Meal {
    pub place: String,
    pub return_to: String,
    pub resume_activity: String,
    pub requested_at: i64,
    pub started_at: Option<i64>,
    pub table: Option<String>,
    pub source_event: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Conversation {
    pub partner: String,
    pub place: String,
    pub until: i64,
    pub resume_activity: String,
    pub event: i64,
}

fn home(id: &str) -> &'static str {
    match id {
        "maya" => "cafe",
        "sam" | "ama" => "home",
        _ => "away",
    }
}

fn food(place: &str) -> &'static str {
    match place {
        "cafe" => "cafe_meals",
        "home" => "home_meals",
        _ => "dorm_meals",
    }
}

impl Sim {
    pub(super) fn life_busy(&self, id: &str) -> bool {
        let life = &self.w.agents[self.ai(id)].life;
        life.meal.is_some() || life.conversation.is_some()
    }

    pub(super) async fn supply_meals(&mut self) {
        // An explicit external supplier boundary; no money or supply-chain simulation.
        for key in ["cafe_meals", "home_meals", "dorm_meals"] {
            let have = self.w.stock.get(key).copied().unwrap_or(0);
            let delivered = (12 - have).max(0);
            self.w.stock.insert(key.into(), have + delivered);
            if delivered > 0 {
                self.event(
                    "world",
                    "food.delivery",
                    key,
                    &format!(
                        "External daily supplier delivered {delivered} portions; stock {}",
                        have + delivered
                    ),
                )
                .await;
            }
        }
    }

    pub(super) async fn tick_life(&mut self) {
        let now = self.now();
        let md = self.md();
        for a in &mut self.w.agents {
            a.life
                .minute(a.activity_kind == "sleeping" && a.travel.is_none());
        }
        self.tick_social().await;
        for k in 0..self.w.agents.len() {
            let id = self.w.agents[k].id.clone();
            if self.w.agents[k].life.conversation.is_some() {
                continue;
            }
            if self.w.agents[k].travel.is_none()
                && matches!(
                    self.w.agents[k].activity_kind.as_str(),
                    "typing" | "studying"
                )
            {
                self.w.agents[k].life.practice_minutes += 1;
                if self.w.agents[k].life.practice_minutes >= 60 {
                    self.w.agents[k].life.practice_minutes = 0;
                    self.w.agents[k].life.practice_sessions =
                        self.w.agents[k].life.practice_sessions.saturating_add(1);
                    let place = self.at(&id);
                    let eid = self
                        .event(
                            &id,
                            "practice.completed",
                            &place,
                            "Completed 60 minutes of situated work or study.",
                        )
                        .await;
                    self.competency(
                        &id,
                        "daily-practice",
                        "focused work",
                        &[format!("event:{eid}")],
                        "practice.v1",
                    )
                    .await;
                }
            }
            if let Some(meal) = self.w.agents[k].life.meal.clone() {
                if !(DAY_START..1290).contains(&md) || now - meal.requested_at >= 90 {
                    self.finish_meal(k, false).await;
                } else if self.w.agents[k].travel.is_none() && self.at(&id) == meal.place {
                    if let Some(start) = meal.started_at {
                        if now >= start + 15 {
                            self.finish_meal(k, true).await;
                        }
                    } else {
                        self.start_meal(k).await;
                    }
                }
                continue;
            }
            if !(DAY_START..1290).contains(&md) {
                if self.w.agents[k].travel.is_none() {
                    if self.at(&id) != home(&id) {
                        self.go(&id, home(&id));
                    } else if md >= DAY_END || md < DAY_START {
                        self.set_activity(&id, "Asleep");
                    } else {
                        self.set_activity(&id, "Winding down for bed");
                    }
                }
                continue;
            }
            if self.w.agents[k].travel.is_some() || now < self.w.agents[k].life.retry_at {
                continue;
            }
            if self.w.agents[k].life.hunger < 60.0 {
                continue;
            }
            let current = self.at(&id);
            let dest = if matches!(current.as_str(), "home" | "away") {
                current.clone()
            } else if self.w.cafe.lifecycle == "open"
                && self.w.stock.get("cafe_meals").copied().unwrap_or(0) > 0
            {
                "cafe".into()
            } else {
                home(&id).into()
            };
            let previous = self.w.agents[k].activity.clone();
            let eid = self
                .event(
                    &id,
                    "meal.seek",
                    &current,
                    &format!(
                        "Hunger {:.1}/100; seek a meal at {}",
                        self.w.agents[k].life.hunger,
                        place_name(&dest)
                    ),
                )
                .await;
            self.decide(&id, "needs.meal.v1", &[format!("event:{eid}")],
                &format!("Eat at {}", place_name(&dest)),
                "Hunger reached 60; choose an open stocked café or the resident's home. Eating requires arrival, stock and a free table.").await;
            self.go(&id, &dest);
            self.w.agents[k].life.meal = Some(Meal {
                place: dest,
                return_to: current,
                resume_activity: previous,
                requested_at: now,
                started_at: None,
                table: None,
                source_event: eid,
            });
            if self.w.agents[k].travel.is_none() {
                self.start_meal(k).await;
            }
        }
    }

    async fn start_meal(&mut self, k: usize) {
        let id = self.w.agents[k].id.clone();
        let meal = self.w.agents[k].life.meal.clone().expect("pending meal");
        let key = food(&meal.place);
        let available = self.w.stock.get(key).copied().unwrap_or(0);
        let owner_break = id == "maya";
        let staff = self.staff_on_duty();
        if available <= 0
            || (meal.place == "cafe"
                && !owner_break
                && (self.w.cafe.lifecycle != "open" || staff.is_none()))
        {
            self.set_activity(&id, "Waiting for food availability");
            if self.now() - meal.requested_at >= 30 {
                self.finish_meal(k, false).await;
            }
            return;
        }
        let table = if meal.place == "cafe" {
            let free = self
                .w
                .cafe
                .tables
                .iter()
                .find(|(_, occupant)| occupant.as_str() == "free")
                .map(|(name, _)| name.clone());
            let Some(table) = free else {
                self.set_activity(&id, "Waiting for a free table");
                return;
            };
            self.w.cafe.tables.insert(table.clone(), id.clone());
            Some(table)
        } else {
            None
        };
        self.w.stock.insert(key.into(), available - 1);
        let eid = self
            .event(
                &id,
                "meal.started",
                &meal.place,
                &format!(
                    "Reserved one {key} portion; eating for 15 minutes; table {}",
                    table.as_deref().unwrap_or("home dining")
                ),
            )
            .await;
        let now = self.now();
        let pending = self.w.agents[k].life.meal.as_mut().unwrap();
        pending.started_at = Some(now);
        pending.table = table;
        pending.source_event = eid;
        self.set_activity(&id, "Eating a meal");
        if let Some(table) = self.w.agents[k]
            .life
            .meal
            .as_ref()
            .and_then(|m| m.table.clone())
        {
            self.w.agents[k].station = table;
        }
    }

    async fn finish_meal(&mut self, k: usize, completed: bool) {
        let id = self.w.agents[k].id.clone();
        let meal = self.w.agents[k].life.meal.take().expect("active meal");
        if let Some(table) = meal.table.as_ref() {
            if self.w.cafe.tables.get(table) == Some(&id) {
                self.w.cafe.tables.insert(table.clone(), "free".into());
            }
        }
        let now = self.now();
        self.w.agents[k].life.retry_at = now + if completed { 30 } else { 60 };
        if completed {
            self.w.agents[k].life.hunger = (self.w.agents[k].life.hunger - 55.0).max(0.0);
            self.w.agents[k].life.meals_completed += 1;
        }
        let detail = if completed {
            "Finished the meal; hunger reduced by 55."
        } else if meal.started_at.is_some() {
            "Meal interrupted; reserved portion discarded, no hunger benefit."
        } else {
            "No meal available before the wait limit; no stock consumed or hunger benefit."
        };
        let eid = self
            .event(
                &id,
                if completed {
                    "meal.consumed"
                } else {
                    "meal.cancelled"
                },
                &meal.place,
                detail,
            )
            .await;
        self.memory(
            &id,
            "temporal",
            "private",
            &meal.place,
            "meal.experience",
            "",
            now + 7 * DAY,
            ("event", eid),
            detail,
        )
        .await;
        self.feed(
            "needs",
            &id,
            if completed {
                "Meal finished"
            } else {
                "Meal unavailable"
            },
            detail,
            vec![
                format!("event:{eid}"),
                format!("event:{}", meal.source_event),
            ],
            "private",
        );
        if (DAY_START..1290).contains(&self.md()) {
            if self.at(&id) != meal.return_to {
                self.go(&id, &meal.return_to);
            } else {
                self.set_activity(&id, &meal.resume_activity);
            }
        }
    }

    async fn tick_social(&mut self) {
        let now = self.now();
        for k in 0..self.w.agents.len() {
            let Some(chat) = self.w.agents[k].life.conversation.clone() else {
                continue;
            };
            let id = self.w.agents[k].id.clone();
            if id > chat.partner {
                continue;
            }
            let other = self.ai(&chat.partner);
            let present = self.at(&id) == chat.place
                && self.at(&chat.partner) == chat.place
                && self.w.agents[k].travel.is_none()
                && self.w.agents[other].travel.is_none();
            if now < chat.until && present {
                continue;
            }
            let completed = present && now >= chat.until;
            for index in [k, other] {
                if let Some(previous) = self.w.agents[index].life.conversation.take() {
                    let who = self.w.agents[index].id.clone();
                    if completed {
                        self.w.agents[index].life.social_need =
                            (self.w.agents[index].life.social_need - 20.0).max(0.0);
                    }
                    self.w.agents[index].life.social_after = now + 60;
                    if self.w.agents[index].travel.is_none() {
                        self.set_activity(&who, &previous.resume_activity);
                    }
                }
            }
            let eid = self
                .event(
                    &id,
                    if completed {
                        "conversation.completed"
                    } else {
                        "conversation.cancelled"
                    },
                    &chat.place,
                    &format!(
                        "{} and {} {} a five-minute conversation",
                        id,
                        chat.partner,
                        if completed {
                            "completed"
                        } else {
                            "interrupted"
                        }
                    ),
                )
                .await;
            if completed {
                let evidence = vec![format!("event:{}", chat.event), format!("event:{eid}")];
                self.interaction(
                    &format!("conversation:{eid}"),
                    &id,
                    &chat.partner,
                    "conversation",
                    "",
                    "invites a neighbour to chat",
                    "accepts while available",
                    "five minutes of shared time",
                    &evidence,
                    "completed",
                )
                .await;
                self.relationship(
                    &format!("daily-social:{id}:{}", chat.partner),
                    &id,
                    &chat.partner,
                    "neighbour",
                    "mutual",
                    "completed a conversation together",
                    "neighborhood",
                    &evidence,
                    "active",
                    60,
                    "social.v1",
                )
                .await;
                for who in [&id, &chat.partner] {
                    self.memory(who, "temporal", "private", &chat.place, "social.experience", "", now + 7 * DAY,
                        ("event", eid), "Enjoyed a five-minute conversation with a neighbour who was present and available.").await;
                }
            }
        }
        if !(DAY_START..1280).contains(&self.md()) {
            return;
        }
        let available = |a: &Agent| {
            a.travel.is_none()
                && a.life.meal.is_none()
                && a.life.conversation.is_none()
                && a.life.social_after <= now
                && a.life.hunger < 60.0
                && matches!(a.location.as_str(), "home" | "park" | "cafe")
                && matches!(
                    a.activity_kind.as_str(),
                    "resting" | "walking" | "meeting" | "idle"
                )
        };
        for k in 0..self.w.agents.len() {
            if !available(&self.w.agents[k]) || self.w.agents[k].life.social_need < 45.0 {
                continue;
            }
            let partner = (0..self.w.agents.len()).find(|&j| {
                j != k
                    && available(&self.w.agents[j])
                    && self.w.agents[k].location == self.w.agents[j].location
            });
            let Some(j) = partner else {
                continue;
            };
            let id = self.w.agents[k].id.clone();
            let other = self.w.agents[j].id.clone();
            let place = self.at(&id);
            let eid = self
                .event(
                    &id,
                    "conversation.started",
                    &place,
                    &format!("{id} and {other} are both free and agree to chat for five minutes."),
                )
                .await;
            for (index, partner) in [(k, other.clone()), (j, id.clone())] {
                let a = &mut self.w.agents[index];
                a.life.conversation = Some(Conversation {
                    partner,
                    place: place.clone(),
                    until: now + 5,
                    resume_activity: a.activity.clone(),
                    event: eid,
                });
                let who = a.id.clone();
                self.set_activity(&who, "Talking with a neighbour");
            }
            self.say(&id, "Good to see you. How has your day been?")
                .await;
            self.say(&other, "I've got a few minutes — let's catch up.")
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_bound_and_sleep_restores_without_feeding() {
        let mut life = Life::default();
        for _ in 0..1440 {
            life.minute(false);
        }
        assert_eq!(life.hunger, 100.0);
        assert_eq!(life.fatigue, 100.0);
        for _ in 0..600 {
            life.minute(true);
        }
        assert!(life.fatigue < 0.001);
        assert_eq!(life.hunger, 100.0);
        assert!((0.0..=100.0).contains(&life.social_need));
    }

    #[test]
    fn restart_preserves_pending_meal_and_exact_need_progress() {
        let mut uninterrupted = Life::default();
        for _ in 0..237 {
            uninterrupted.minute(false);
        }
        uninterrupted.meal = Some(Meal {
            place: "cafe".into(),
            return_to: "school".into(),
            resume_activity: "Studying".into(),
            requested_at: 600,
            started_at: Some(612),
            table: Some("table1".into()),
            source_event: 42,
        });
        let mut resumed: Life =
            serde_json::from_str(&serde_json::to_string(&uninterrupted).unwrap()).unwrap();
        for _ in 0..271 {
            uninterrupted.minute(false);
            resumed.minute(false);
        }
        assert_eq!(uninterrupted, resumed);
    }

    #[tokio::test]
    async fn scarce_food_is_reserved_once_and_only_completed_eating_changes_hunger() {
        let (store, mut w, dir) = crate::scenarios::fixture().await;
        w.minute = 700;
        for a in &mut w.agents {
            a.life.hunger = 0.0;
        }
        w.stock.insert("home_meals".into(), 1);
        for id in ["sam", "ama"] {
            w.agents
                .iter_mut()
                .find(|a| a.id == id)
                .unwrap()
                .life
                .hunger = 70.0;
        }
        let mut sim = Sim {
            store: store.clone(),
            w,
            llm: None,
        };
        sim.tick_life().await;
        assert_eq!(sim.w.stock["home_meals"], 0);
        assert_eq!(
            sim.w
                .agents
                .iter()
                .filter(|a| a.life.meal.as_ref().is_some_and(|m| m.started_at.is_some()))
                .count(),
            1
        );
        let sam = sim.ai("sam");
        let ama = sim.ai("ama");
        assert!(
            sim.w.agents[sam].life.hunger > 70.0,
            "intending to eat is not nutrition"
        );
        for _ in 0..14 {
            sim.w.minute += 1;
            sim.tick_life().await;
        }
        assert_eq!(sim.w.agents[sam].life.meals_completed, 0);
        sim.persist().await;
        let rows = store
            .run(
                sim.now(),
                "test",
                "reload pending meal",
                "SELECT v FROM lv_kv WHERE k = 'world' ORDER BY sim_minute DESC LIMIT 1",
            )
            .await
            .unwrap();
        let resumed: World = serde_json::from_str(&s(&rows[0], "v")).unwrap();
        assert_eq!(resumed.agents[sam].life, sim.w.agents[sam].life);
        sim.w = resumed;
        sim.w.minute += 1;
        sim.tick_life().await;
        assert_eq!(sim.w.agents[sam].life.meals_completed, 1);
        assert!(sim.w.agents[sam].life.hunger < 20.0);
        assert!(sim.w.agents[ama].life.hunger > 70.0);
        for _ in 0..20 {
            sim.w.minute += 1;
            sim.tick_life().await;
        }
        assert_eq!(
            sim.w.agents[sam].life.meals_completed, 1,
            "no duplicate consumption"
        );
        let events = store
            .run(
                sim.now(),
                "test",
                "meal evidence",
                "SELECT kind FROM lv_events WHERE kind = 'meal.consumed'",
            )
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        drop(sim);
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn coffee_queue_has_time_stock_and_presence_constraints() {
        let (store, mut w, dir) = crate::scenarios::fixture().await;
        w.minute = 500;
        w.stock.insert("oat_milk".into(), 1);
        let mut sim = Sim {
            store: store.clone(),
            w,
            llm: None,
        };
        let lin = sim.ai("lin");
        sim.w.agents[lin].location = "cafe".into();
        sim.counter_order("lin", "oat latte", "oat_milk").await;
        sim.counter_order("lin", "oat latte", "oat_milk").await;
        assert_eq!(sim.w.cafe.queue.len(), 1);
        sim.tick_orders().await;
        assert_eq!(sim.w.stock["oat_milk"], 0);
        assert!(sim.w.agents[lin].carrying.is_none());
        sim.w.minute += 2;
        sim.tick_orders().await;
        assert!(sim.w.agents[lin].carrying.is_none());
        sim.w.minute += 1;
        sim.tick_orders().await;
        assert_eq!(sim.w.agents[lin].carrying.as_deref(), Some("coffee"));
        assert_eq!(
            sim.w.cafe.active_order.as_ref().unwrap().status,
            "handed_off"
        );
        assert_eq!(sim.w.stock["oat_milk"], 0);
        sim.w.agents[lin].carrying = None;
        sim.w.minute += 1;
        sim.counter_order("lin", "oat latte", "oat_milk").await;
        sim.tick_orders().await;
        assert_eq!(sim.w.cafe.active_order.as_ref().unwrap().status, "declined");
        assert!(
            sim.w.agents[lin].carrying.is_none(),
            "no unsuitable alternative for Lin"
        );
        drop(sim);
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn social_completion_requires_five_minutes_and_shared_place() {
        let (store, mut w, dir) = crate::scenarios::fixture().await;
        w.minute = 1000;
        let mut sim = Sim {
            store: store.clone(),
            w,
            llm: None,
        };
        for id in ["sam", "ama"] {
            let k = sim.ai(id);
            sim.w.agents[k].life.social_need = 60.0;
            sim.set_activity(id, "Resting at home");
        }
        sim.tick_social().await;
        let sam = sim.ai("sam");
        let ama = sim.ai("ama");
        assert!(sim.w.agents[sam].life.conversation.is_some());
        sim.w.minute += 4;
        sim.tick_social().await;
        assert_eq!(sim.w.agents[sam].life.social_need, 60.0);
        sim.w.agents[ama].location = "park".into();
        sim.tick_social().await;
        assert!(sim.w.agents[sam].life.conversation.is_none());
        assert_eq!(
            sim.w.agents[sam].life.social_need, 60.0,
            "interruption grants no benefit"
        );
        sim.w.minute += 61;
        sim.w.agents[ama].location = "home".into();
        sim.tick_social().await;
        sim.w.minute += 5;
        sim.tick_social().await;
        assert_eq!(sim.w.agents[sam].life.social_need, 40.0);
        assert_eq!(sim.w.agents[ama].life.social_need, 40.0);
        let rows = store
            .run(
                sim.now(),
                "test",
                "social evidence",
                "SELECT * FROM lv_interactions WHERE kind = 'conversation'",
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        drop(sim);
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn overnight_is_continuous_and_sleep_restores_at_home() {
        let (store, mut w, dir) = crate::scenarios::fixture().await;
        w.minute = DAY_END - 1;
        let mut sim = Sim {
            store: store.clone(),
            w,
            llm: None,
        };
        let sam = sim.ai("sam");
        sim.w.agents[sam].life.fatigue = 80.0;
        sim.advance().await;
        assert_eq!(sim.now(), DAY_END);
        assert_eq!(sim.w.agents[sam].activity_kind, "sleeping");
        for _ in 0..60 {
            let before = sim.now();
            sim.advance().await;
            assert_eq!(sim.now(), before + 1);
        }
        assert!(sim.w.agents[sam].life.fatigue < 71.0);
        assert_eq!(sim.w.agents[sam].location, "home");
        drop(sim);
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn supporting_population_lives_a_day_and_replays_exactly() {
        let (store, w, dir) = crate::scenarios::fixture().await;
        let mut sim = Sim {
            store: store.clone(),
            w: w.clone(),
            llm: None,
        };
        assert_eq!(sim.w.agents.len() + sim.w.background.len(), 100);
        assert_eq!(
            sim.w
                .background
                .iter()
                .map(|a| &a.id)
                .collect::<BTreeSet<_>>()
                .len(),
            95
        );
        for _ in 0..DAY {
            sim.w.minute += 1;
            sim.tick_background();
        }
        for person in &sim.w.background {
            assert!(person.life.meals_completed >= 1, "{} never ate", person.id);
            assert!(
                person.life.practice_sessions >= 1,
                "{} never worked",
                person.id
            );
            assert!(person.memories.len() <= 12);
            assert!(
                person.travel_until <= sim.now(),
                "{} is still travelling at dawn",
                person.id
            );
            assert!((0.0..=100.0).contains(&person.life.hunger));
            assert!((0.0..=100.0).contains(&person.life.fatigue));
        }
        let expected = serde_json::to_value(&sim.w.background).unwrap();
        sim.w = w;
        for _ in 0..DAY {
            sim.w.minute += 1;
            sim.tick_background();
        }
        assert_eq!(expected, serde_json::to_value(&sim.w.background).unwrap());
        drop(sim);
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn close_traffic_leaders_do_not_yield_to_followers() {
        let (store, w, dir) = crate::scenarios::fixture().await;
        let mut sim = Sim {
            store: store.clone(),
            w,
            llm: None,
        };
        for direction in ["east", "west"] {
            sim.w.traffic.truncate(2);
            let sign = if direction == "east" { 1.0 } else { -1.0 };
            for (i, actor) in sim.w.traffic.iter_mut().enumerate() {
                actor.direction = direction.into();
                actor.lane = "test-lane".into();
                actor.position = 20.0 + sign * i as f32 * 5.0;
            }
            let start: Vec<_> = sim.w.traffic.iter().map(|a| a.position).collect();
            sim.tick_traffic();
            assert!((sim.w.traffic[1].position - start[1]) * sign > 0.0);
            assert_eq!(sim.w.traffic[0].position, start[0]);
            for _ in 0..100 {
                sim.tick_traffic();
            }
            assert!((sim.w.traffic[0].position - start[0]) * sign > 5.0);
        }
        drop(sim);
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn neighborhood_population_stays_within_street_capacity_for_a_week() {
        let (store, w, dir) = crate::scenarios::fixture().await;
        let mut sim = Sim {
            store: store.clone(),
            w,
            llm: None,
        };
        let mut peak = 0;
        for _ in 0..7 * DAY {
            sim.w.minute += 1;
            sim.tick_background();
            let count = sim
                .w
                .background
                .iter()
                .filter(|b| b.visible_in_town(sim.now()))
                .count();
            peak = peak.max(count);
            assert!(
                count <= 8,
                "{} supporting residents outdoors at {}",
                count,
                sim.now()
            );
            assert_eq!(sim.w.background.len(), 95);
        }
        assert!(peak >= 4, "capacity policy must not empty the town");
        println!("Neighborhood outdoor peak over seven days: {peak}");
        assert!(sim.w.background.iter().all(|b| b.life.meals_completed >= 7));
        drop(sim);
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn versioned_checkpoint_survives_flush_merge_and_same_minute_updates() {
        let (store, w, dir) = crate::scenarios::fixture().await;
        let mut sim = Sim {
            store: store.clone(),
            w,
            llm: None,
        };
        sim.w.minute = 1000;
        sim.w.paused = false;
        sim.persist().await;
        store.engine.flush("lv_kv").unwrap();
        sim.w.minute = 2440;
        sim.persist().await;
        store.engine.flush("lv_kv").unwrap();
        sim.w.paused = true;
        sim.persist().await;
        store.engine.flush("lv_kv").unwrap();
        store.engine.optimize("lv_kv").unwrap();
        let rows = store.run(sim.now(), "test", "resume newest revision",
            "SELECT v FROM lv_kv WHERE k = 'world' OR k LIKE 'world:%' ORDER BY sim_minute DESC, k DESC LIMIT 1"
        ).await.unwrap();
        let resumed: World = serde_json::from_str(&s(&rows[0], "v")).unwrap();
        assert_eq!(resumed.minute, 2440);
        assert!(
            resumed.paused,
            "same-minute control update must survive compaction"
        );
        assert_eq!(resumed.agents[0].life, sim.w.agents[0].life);
        assert_eq!(resumed.background.len(), 95);
        for _ in 0..65 {
            sim.persist().await;
        }
        let counts = store
            .run(
                sim.now(),
                "test",
                "bounded checkpoint history",
                "SELECT COUNT(*) AS n FROM lv_kv WHERE k LIKE 'world:%'",
            )
            .await
            .unwrap();
        assert_eq!(i(&counts[0], "n"), 64);
        drop(sim);
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
