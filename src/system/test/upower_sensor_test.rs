use crate::{
    external::dbus::ConnectionFactory,
    system::upower_sensor::{PowerStatus, UPowerSensor},
};

// Another function that's not an actual test but rather more similar to a simulation
// Start it with the computer connected to external power
// #[tokio::test]
// #[ignore]
// async fn interactive_upower_test() {
//     let mut connection_factory = ConnectionFactory::new();
//     let mut receive_channel =
//         UPowerSensor::new(connection_factory.get_system().await.unwrap())
//             .await
//             .unwrap();
//     assert_eq!(*receive_channel.borrow_and_update(), PowerStatus::External);
//     println!("Please disconnect the external power source");
//     receive_channel.changed().await.unwrap();
//     assert_eq!(*receive_channel.borrow_and_update(), PowerStatus::Battery);
//     println!("Please connect the external power source");
//     receive_channel.changed().await.unwrap();
//     assert_eq!(*receive_channel.borrow_and_update(), PowerStatus::External);
// }
