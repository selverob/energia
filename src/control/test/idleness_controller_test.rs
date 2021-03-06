use std::{
    cell::Cell,
    collections::HashSet,
    sync::{Arc, Mutex},
};

use logind_zbus::manager::{InhibitType, InhibitTypes, Inhibitor, Mode};

use crate::{
    armaf::{spawn_server, ActorPort, Effect, EffectorMessage, EffectorPort, RollbackStrategy},
    control::idleness_controller::{Action, IdlenessController, ReconciliationBunches},
    external::display_server::SystemState,
    system::inhibition_sensor::GetInhibitions,
};

use super::effects_counter::EffectsCounter;

struct MockInhibitionSensor {
    inhibitors: Arc<Mutex<Cell<Vec<Inhibitor>>>>,
}

impl MockInhibitionSensor {
    fn new() -> MockInhibitionSensor {
        MockInhibitionSensor {
            inhibitors: Arc::new(Mutex::new(Cell::new(Vec::new()))),
        }
    }

    fn add_inhibitor_with_types(&self, mode: Mode, ts: &Vec<InhibitType>) {
        let inhibit_types = InhibitTypes::new(ts);
        let inhibitor_count = self.inhibitors.lock().unwrap().get_mut().len();
        let inhibitor = Inhibitor::new(
            inhibit_types,
            format!("Inhibitor{}", inhibitor_count),
            "Testing".to_owned(),
            mode,
            0,
            0,
        );
        self.inhibitors.lock().unwrap().get_mut().push(inhibitor);
    }

    fn reset(&self) {
        *self.inhibitors.lock().unwrap().get_mut() = Vec::new();
    }

    fn spawn(&self) -> ActorPort<GetInhibitions, Vec<Inhibitor>, anyhow::Error> {
        let (port, mut rx) = ActorPort::make();

        let inhibitors = self.inhibitors.clone();

        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                req.respond(Ok(inhibitors.lock().unwrap().get_mut().clone()))
                    .unwrap();
            }
        });

        port
    }
}

fn make_action(
    bunch: usize,
    effect_no: usize,
    port: EffectorPort,
    rollback: RollbackStrategy,
) -> Action {
    Action::new(
        Effect::new(format!("{}-{}", bunch, effect_no), vec![], rollback),
        port,
    )
}

#[tokio::test]
async fn test_without_inhibitors() {
    let ec1 = EffectsCounter::new();
    let ec2 = EffectsCounter::new();
    let ec3 = EffectsCounter::new();
    let action_bunches = vec![
        vec![
            make_action(1, 1, ec1.get_port(), RollbackStrategy::OnActivity),
            make_action(1, 2, ec2.get_port(), RollbackStrategy::OnActivity),
        ],
        vec![
            make_action(2, 1, ec1.get_port(), RollbackStrategy::OnActivity),
            make_action(2, 2, ec2.get_port(), RollbackStrategy::Immediate),
        ],
        vec![
            make_action(1, 1, ec1.get_port(), RollbackStrategy::Immediate),
            make_action(1, 2, ec2.get_port(), RollbackStrategy::OnActivity),
            make_action(1, 3, ec3.get_port(), RollbackStrategy::OnActivity),
        ],
    ];

    let inhibition_sensor = MockInhibitionSensor::new();
    let idleness_controller = IdlenessController::new(
        action_bunches,
        0,
        ReconciliationBunches::new(None, None, HashSet::new()),
        inhibition_sensor.spawn(),
    );
    let controller_port = spawn_server(idleness_controller).await.unwrap();
    // Moving to bunch 0
    controller_port.request(SystemState::Idle).await.unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 1);
    assert_eq!(ec2.ongoing_effect_count(), 1);
    assert_eq!(ec3.ongoing_effect_count(), 0);

    // Rolling back
    controller_port
        .request(SystemState::Awakened)
        .await
        .unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 0);
    assert_eq!(ec2.ongoing_effect_count(), 0);
    assert_eq!(ec3.ongoing_effect_count(), 0);

    // Moving to bunch 0
    controller_port.request(SystemState::Idle).await.unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 1);
    assert_eq!(ec2.ongoing_effect_count(), 1);
    assert_eq!(ec3.ongoing_effect_count(), 0);

    // Moving to bunch 1
    controller_port.request(SystemState::Idle).await.unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 2);
    assert_eq!(ec2.ongoing_effect_count(), 1);
    assert_eq!(ec3.ongoing_effect_count(), 0);

    // Moving to bunch 2
    controller_port.request(SystemState::Idle).await.unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 2);
    assert_eq!(ec2.ongoing_effect_count(), 2);
    assert_eq!(ec3.ongoing_effect_count(), 1);

    // Rolling back
    controller_port
        .request(SystemState::Awakened)
        .await
        .unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 0);
    assert_eq!(ec2.ongoing_effect_count(), 0);
    assert_eq!(ec3.ongoing_effect_count(), 0);
}

#[tokio::test]
async fn test_inhibitions() {
    let ec1 = EffectsCounter::new();
    let ec2 = EffectsCounter::new();
    let action_bunches = vec![vec![
        Action::new(
            Effect::new(
                "1-1".to_owned(),
                vec![InhibitType::Shutdown, InhibitType::Sleep],
                RollbackStrategy::OnActivity,
            ),
            ec1.get_port(),
        ),
        Action::new(
            Effect::new(
                "1-2".to_owned(),
                vec![InhibitType::Idle],
                RollbackStrategy::OnActivity,
            ),
            ec2.get_port(),
        ),
    ]];

    let inhibition_sensor = MockInhibitionSensor::new();
    let idleness_controller = IdlenessController::new(
        action_bunches,
        0,
        ReconciliationBunches::new(None, None, HashSet::new()),
        inhibition_sensor.spawn(),
    );
    let controller_port = spawn_server(idleness_controller).await.unwrap();

    // Moving to bunch 0, shouldn't be inhibited, Delay inhibitors are ignored
    inhibition_sensor.add_inhibitor_with_types(Mode::Delay, &vec![InhibitType::Sleep]);
    inhibition_sensor
        .add_inhibitor_with_types(Mode::Delay, &vec![InhibitType::Shutdown, InhibitType::Idle]);
    controller_port.request(SystemState::Idle).await.unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 1);
    assert_eq!(ec2.ongoing_effect_count(), 1);

    // Rolling back
    controller_port
        .request(SystemState::Awakened)
        .await
        .unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 0);
    assert_eq!(ec2.ongoing_effect_count(), 0);

    // Should not move to bunch 0, inhibited
    inhibition_sensor.reset();
    inhibition_sensor.add_inhibitor_with_types(Mode::Block, &vec![InhibitType::Sleep]);
    controller_port
        .request(SystemState::Idle)
        .await
        .expect_err("Bunch applied even when inhibited");
    assert_eq!(ec1.ongoing_effect_count(), 0);
    assert_eq!(ec2.ongoing_effect_count(), 0);

    // Should not move to bunch 0, inhibited - testing multiple overlapping inhibitors
    inhibition_sensor.reset();
    inhibition_sensor.add_inhibitor_with_types(Mode::Block, &vec![InhibitType::Sleep]);
    inhibition_sensor
        .add_inhibitor_with_types(Mode::Block, &vec![InhibitType::Sleep, InhibitType::Idle]);
    controller_port
        .request(SystemState::Idle)
        .await
        .expect_err("Bunch applied even when inhibited");
    assert_eq!(ec1.ongoing_effect_count(), 0);
    assert_eq!(ec2.ongoing_effect_count(), 0);

    // Move to bunch 0, unrelated inhibitors
    inhibition_sensor.reset();
    inhibition_sensor.add_inhibitor_with_types(Mode::Block, &vec![InhibitType::HandleHibernateKey]);
    inhibition_sensor.add_inhibitor_with_types(Mode::Block, &vec![InhibitType::HandleLidSwitch]);
    controller_port.request(SystemState::Idle).await.unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 1);
    assert_eq!(ec2.ongoing_effect_count(), 1);

    // Rollback should not be affected by inhibitors
    inhibition_sensor.reset();
    inhibition_sensor.add_inhibitor_with_types(Mode::Block, &vec![InhibitType::Sleep]);
    controller_port
        .request(SystemState::Awakened)
        .await
        .unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 0);
    assert_eq!(ec2.ongoing_effect_count(), 0);
}

#[tokio::test]
async fn test_reconciliation() {
    let ec1 = EffectsCounter::new();
    let rec1 = EffectsCounter::new();
    let rec2 = EffectsCounter::new();

    let action_bunches = vec![
        vec![make_action(
            1,
            1,
            ec1.get_port(),
            RollbackStrategy::OnActivity,
        )],
        vec![make_action(
            2,
            1,
            ec1.get_port(),
            RollbackStrategy::OnActivity,
        )],
        vec![make_action(
            3,
            1,
            ec1.get_port(),
            RollbackStrategy::OnActivity,
        )],
        vec![make_action(
            4,
            1,
            ec1.get_port(),
            RollbackStrategy::OnActivity,
        )],
    ];

    let reconciliation = ReconciliationBunches::new(
        Some(vec![
            Action::new(
                Effect::new(
                    "1-1".to_owned(),
                    vec![InhibitType::Idle],
                    RollbackStrategy::OnActivity,
                ),
                rec1.get_port(),
            ),
            make_action(1, 2, rec1.get_port(), RollbackStrategy::OnActivity),
        ]),
        Some(vec![rec2.get_port()]),
        HashSet::new(),
    );

    rec2.get_port()
        .request(EffectorMessage::Execute)
        .await
        .unwrap();
    let inhibition_sensor = MockInhibitionSensor::new();
    let idleness_controller =
        IdlenessController::new(action_bunches, 1, reconciliation, inhibition_sensor.spawn());
    let controller_port = spawn_server(idleness_controller).await.unwrap();

    inhibition_sensor.add_inhibitor_with_types(Mode::Block, &vec![InhibitType::Idle]);
    controller_port
        .request(SystemState::Idle)
        .await
        .expect_err("Bunch applied even when inhibited");
    assert_eq!(ec1.ongoing_effect_count(), 0);
    assert_eq!(rec1.ongoing_effect_count(), 0);
    assert_eq!(rec2.ongoing_effect_count(), 1);

    inhibition_sensor.reset();
    controller_port.request(SystemState::Idle).await.unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 1);
    assert_eq!(rec1.ongoing_effect_count(), 2);
    assert_eq!(rec2.ongoing_effect_count(), 1);

    controller_port.request(SystemState::Idle).await.unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 2);
    assert_eq!(rec1.ongoing_effect_count(), 2);
    assert_eq!(rec2.ongoing_effect_count(), 1);

    controller_port
        .request(SystemState::Awakened)
        .await
        .unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 0);
    assert_eq!(rec1.ongoing_effect_count(), 0);
    assert_eq!(rec2.ongoing_effect_count(), 0);
}

#[tokio::test]
async fn test_rollback_on_zero_position() {
    let ec1 = EffectsCounter::new();
    let rec1 = EffectsCounter::new();

    let action_bunches = vec![vec![make_action(
        1,
        1,
        ec1.get_port(),
        RollbackStrategy::OnActivity,
    )]];

    let reconciliation =
        ReconciliationBunches::new(None, Some(vec![rec1.get_port()]), HashSet::new());

    rec1.get_port()
        .request(EffectorMessage::Execute)
        .await
        .unwrap();
    let inhibition_sensor = MockInhibitionSensor::new();
    let idleness_controller =
        IdlenessController::new(action_bunches, 0, reconciliation, inhibition_sensor.spawn());
    let _controller_port = spawn_server(idleness_controller).await.unwrap();

    assert_eq!(ec1.ongoing_effect_count(), 0);
    assert_eq!(rec1.ongoing_effect_count(), 0);
    _controller_port.await_shutdown().await;
}

#[tokio::test]
async fn test_effect_skipping() {
    let ec1 = EffectsCounter::new();
    let ec2 = EffectsCounter::new();

    let action_bunches = vec![
        vec![
            make_action(1, 1, ec1.get_port(), RollbackStrategy::OnActivity),
            make_action(1, 2, ec2.get_port(), RollbackStrategy::OnActivity),
        ],
        vec![
            make_action(2, 1, ec1.get_port(), RollbackStrategy::OnActivity),
            make_action(2, 2, ec2.get_port(), RollbackStrategy::OnActivity),
        ],
    ];

    let inhibition_sensor = MockInhibitionSensor::new();
    let skip_set = HashSet::from(["1-1".to_owned(), "2-2".to_owned()]);
    let idleness_controller = IdlenessController::new(
        action_bunches,
        0,
        ReconciliationBunches::new(None, None, skip_set),
        inhibition_sensor.spawn(),
    );
    let controller_port = spawn_server(idleness_controller).await.unwrap();

    controller_port.request(SystemState::Idle).await.unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 0);
    assert_eq!(ec2.ongoing_effect_count(), 1);

    controller_port.request(SystemState::Idle).await.unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 1);
    assert_eq!(ec2.ongoing_effect_count(), 1);

    controller_port
        .request(SystemState::Awakened)
        .await
        .unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 0);
    assert_eq!(ec2.ongoing_effect_count(), 0);

    controller_port.request(SystemState::Idle).await.unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 1);
    assert_eq!(ec2.ongoing_effect_count(), 1);

    controller_port.request(SystemState::Idle).await.unwrap();
    assert_eq!(ec1.ongoing_effect_count(), 2);
    assert_eq!(ec2.ongoing_effect_count(), 2);
}
