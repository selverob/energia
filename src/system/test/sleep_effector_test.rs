// This is indeed not a true test. It's a bit difficult to test behavior which
// actually causes the computer to go down.
// If you want to test if the functionality didn't break, just uncomment it,
// run it manually and keep the computer asleep for at least 10 seconds.

// use std::env;
// use std::time::SystemTime;

// use crate::{
//     armaf::{spawn_server, EffectorMessage},
//     external::dbus,
//     system::sleep_effector,
// };

// #[tokio::test]
// #[ignore]
// async fn test_idle_hints() {
//     env::set_var("RUST_LOG", "debug");
//     env_logger::init();
//     let mut factory = dbus::ConnectionFactory::new();
//     let port = spawn_server(sleep_effector::SleepEffector::new(
//         factory.get_system().await.unwrap(),
//     ))
//     .await
//     .expect("Failed to start actor");
//     port.request(EffectorMessage::Execute)
//         .await
//         .expect("Failed to put computer to sleep");
//     // Instant:: is a sythetic monotonic clock - it stops in sleep, so it will always just give you 5 seconds
//     let start = SystemTime::now();
//     port.request(EffectorMessage::Rollback)
//         .await
//         .expect("Failed to put computer to sleep");
//     let elapsed_time = start.elapsed().unwrap();
//     log::debug!("Rollback done after {}ms", elapsed_time.as_millis());
//     assert!(elapsed_time.as_secs() > 10);
// }
