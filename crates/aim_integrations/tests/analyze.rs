use aim_integrations::{
    Analysis, Error, MatchReport, OpeningDuels, PlayerSelector, RIOT_NOTICE, RangeCounts,
    WeaponUse, analyze, analyze_json, parse_match, recommend,
};

const FIXTURE: &str = include_str!("fixtures/match.json");

fn report(player: &str) -> MatchReport {
    analyze_json(FIXTURE, player).expect("fixture analyzes")
}

fn rules(analysis: &Analysis) -> Vec<&'static str> {
    recommend(analysis).iter().map(|i| i.rule).collect()
}

#[test]
fn computes_every_number_for_the_selected_player() {
    let r = report("p-me");
    assert_eq!(r.match_id, "fixture-match-0001");
    assert_eq!(r.map_id, "/Game/Maps/Ascent/Ascent");
    assert_eq!(r.rounds, 4);
    assert_eq!(r.team_won, Some(true));
    assert_eq!(r.player.game_name, "Ace");
    assert_eq!(r.player.character_id, "jett-uuid");

    let a = &r.analysis;
    assert_eq!((a.kills, a.deaths, a.assists), (5, 3, 1));
    assert_eq!((a.headshots, a.bodyshots, a.legshots), (3, 8, 1));
    assert_eq!(a.headshot_rate(), Some(0.25));
    assert_eq!(
        a.kill_distances_m,
        vec![Some(10.0), Some(30.0), Some(7.0), Some(25.0), None]
    );
    assert_eq!(a.death_distances_m, vec![Some(50.0), Some(5.0), Some(20.0)]);
    assert_eq!(
        a.kills_by_range,
        RangeCounts {
            close: 1,
            mid: 2,
            long: 1,
            unknown: 1
        }
    );
    assert_eq!(
        a.deaths_by_range,
        RangeCounts {
            close: 1,
            mid: 1,
            long: 1,
            unknown: 0
        }
    );
    assert_eq!(a.opening_duels, OpeningDuels { won: 2, lost: 1 });
    assert_eq!(a.avg_first_kill_secs, Some(15.0));
    let weapon = |damage_type: &str, item: &str, kills| WeaponUse {
        damage_type: damage_type.to_owned(),
        item: item.to_owned(),
        kills,
    };
    assert_eq!(
        a.weapon_usage,
        vec![
            weapon("Weapon", "vandal-uuid", 3),
            weapon("Weapon", "sheriff-uuid", 1),
            weapon("Ability", "ultimate", 1),
        ]
    );
    assert!(r.insights.is_empty(), "{:?}", r.insights);
}

#[test]
fn selects_a_player_by_riot_id_case_insensitively() {
    let by_id = report("Ace#EUW");
    assert_eq!(by_id.player.puuid, "p-me");
    assert_eq!(report("ace#euw"), by_id);
    assert_eq!(
        PlayerSelector::parse("Ace#EUW").expect("valid"),
        PlayerSelector::RiotId {
            game_name: "Ace".to_owned(),
            tag_line: "EUW".to_owned()
        }
    );
}

#[test]
fn another_player_gets_their_own_numbers_and_insight() {
    let r = report("Rival#NA1");
    let a = &r.analysis;
    assert_eq!(r.team_won, Some(false));
    assert_eq!((a.kills, a.deaths), (2, 2));
    assert_eq!((a.headshots, a.bodyshots, a.legshots), (0, 10, 2));
    assert_eq!(a.kill_distances_m, vec![Some(50.0), Some(20.0)]);
    assert_eq!(a.death_distances_m, vec![Some(30.0), Some(25.0)]);
    assert_eq!(a.opening_duels, OpeningDuels::default());
    assert_eq!(a.avg_first_kill_secs, Some(21.0));
    assert_eq!(r.insights.len(), 1);
    let insight = &r.insights[0];
    assert_eq!(insight.rule, "low-headshot-rate");
    assert_eq!(insight.value, 0.0);
    assert_eq!(insight.threshold, 0.20);
    assert_eq!(insight.drills, ["static-headline", "flick-headshot"]);
}

#[test]
fn unknown_player_is_an_error() {
    assert!(matches!(
        analyze_json(FIXTURE, "Nobody#000"),
        Err(Error::PlayerNotFound(p)) if p == "Nobody#000"
    ));
    assert!(matches!(
        analyze_json(FIXTURE, "p-missing"),
        Err(Error::PlayerNotFound(_))
    ));
    for bad in ["", "  ", "#EUW", "Ace#"] {
        assert!(
            matches!(analyze_json(FIXTURE, bad), Err(Error::InvalidSelector(_))),
            "{bad:?}"
        );
    }
}

#[test]
fn malformed_json_is_an_error_not_a_panic() {
    for bad in [
        "",
        "{",
        "null",
        r#"{"players": 3}"#,
        r#"{"roundResults": [{"playerStats": [{"kills": [{"timeSinceRoundStartMillis": -1}]}]}]}"#,
    ] {
        assert!(
            matches!(analyze_json(bad, "p-me"), Err(Error::Json(_))),
            "{bad:?}"
        );
    }
}

#[test]
fn missing_and_unknown_fields_are_tolerated() {
    let m = parse_match(r#"{"players": [{"puuid": "x", "extra": {"a": 1}}], "teams": null}"#)
        .expect("tolerant");
    let r = analyze(&m, &PlayerSelector::Puuid("x".to_owned())).expect("found");
    assert_eq!(r.rounds, 0);
    assert_eq!(r.team_won, None);
    assert_eq!(r.analysis.headshot_rate(), None);
    assert_eq!(r.analysis.avg_first_kill_secs, None);
    assert!(r.insights.is_empty());
}

#[test]
fn text_report_shows_the_numbers_and_the_riot_notice() {
    let text = report("p-me").to_text();
    for expected in [
        "K/D/A: 5/3/1 (K/D 1.67)",
        "Headshot rate of hits: 25.0% (3 head, 8 body, 1 legs; shots fired are not in the data)",
        "Kills by range (approx.): close <10 m 1, mid 10-25 m 2, long >25 m 1, unknown 1",
        "Opening duels: won 2, lost 1",
        "Average time to first kill in a round: 15.0 s",
        "Weapon vandal-uuid: 3",
        "No rule fired",
        RIOT_NOTICE,
    ] {
        assert!(text.contains(expected), "missing {expected:?} in\n{text}");
    }
    assert!(!text.to_lowercase().contains("accuracy"));
    let rival = report("Rival#NA1").to_text();
    assert!(rival.contains("[low-headshot-rate]"), "{rival}");
    assert!(
        rival.contains("Drills: static-headline, flick-headshot"),
        "{rival}"
    );
}

// Insight thresholds. Shares are compared strictly, so a value equal to the threshold
// does not fire.

fn with_hits(head: u32, body: u32) -> Analysis {
    Analysis {
        headshots: head,
        bodyshots: body,
        ..Analysis::default()
    }
}

#[test]
fn low_headshot_rule_fires_below_twenty_percent_with_enough_hits() {
    assert_eq!(rules(&with_hits(19, 81)), ["low-headshot-rate"]);
    assert!(rules(&with_hits(20, 80)).is_empty());
    assert!(rules(&with_hits(0, 9)).is_empty(), "needs 10 hits");
    assert_eq!(rules(&with_hits(0, 10)), ["low-headshot-rate"]);
}

fn with_deaths(close: u32, mid: u32, long: u32, unknown: u32) -> Analysis {
    Analysis {
        deaths_by_range: RangeCounts {
            close,
            mid,
            long,
            unknown,
        },
        ..Analysis::default()
    }
}

#[test]
fn long_range_rule_fires_above_half_of_known_deaths() {
    assert_eq!(rules(&with_deaths(0, 1, 2, 0)), ["long-range-deaths"]);
    assert!(rules(&with_deaths(1, 1, 2, 0)).is_empty(), "exactly 50%");
    assert!(rules(&with_deaths(0, 0, 2, 5)).is_empty(), "needs 3 known");
}

#[test]
fn close_range_rule_fires_above_half_of_known_deaths() {
    assert_eq!(rules(&with_deaths(2, 1, 0, 0)), ["close-range-deaths"]);
    assert!(rules(&with_deaths(2, 1, 1, 0)).is_empty(), "exactly 50%");
    assert!(rules(&with_deaths(2, 0, 0, 0)).is_empty(), "needs 3 known");
}

fn with_openings(won: u32, lost: u32) -> Analysis {
    Analysis {
        opening_duels: OpeningDuels { won, lost },
        ..Analysis::default()
    }
}

#[test]
fn opening_rule_fires_above_sixty_percent_lost() {
    assert_eq!(rules(&with_openings(1, 2)), ["opening-duels-lost"]);
    assert!(rules(&with_openings(2, 3)).is_empty(), "exactly 60%");
    assert!(rules(&with_openings(0, 2)).is_empty(), "needs 3 duels");
}

#[test]
fn rules_run_in_a_fixed_order() {
    let a = Analysis {
        headshots: 1,
        bodyshots: 20,
        opening_duels: OpeningDuels { won: 0, lost: 3 },
        deaths_by_range: RangeCounts {
            close: 0,
            mid: 0,
            long: 3,
            unknown: 0,
        },
        ..Analysis::default()
    };
    assert_eq!(
        rules(&a),
        [
            "low-headshot-rate",
            "long-range-deaths",
            "opening-duels-lost"
        ]
    );
}

#[test]
fn every_recommended_drill_is_a_builtin_scenario() {
    let ids: Vec<String> = aim_core::scenario::builtin::all()
        .into_iter()
        .map(|s| s.id)
        .collect();
    let a = Analysis {
        headshots: 0,
        bodyshots: 20,
        opening_duels: OpeningDuels { won: 0, lost: 3 },
        ..Analysis::default()
    };
    let mut insights = recommend(&a);
    insights.extend(recommend(&with_deaths(0, 0, 3, 0)));
    insights.extend(recommend(&with_deaths(3, 0, 0, 0)));
    assert_eq!(insights.len(), 4);
    for drill in insights.iter().flat_map(|i| i.drills) {
        assert!(ids.iter().any(|id| id == drill), "{drill} is not built in");
    }
}
