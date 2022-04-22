use crate::{
    external::dbus::ConnectionFactory,
    system::upower_sensor::{PowerStatus, UPowerSensor},
};

//Only a semi-automated test
//Start it with the computer connected to external power
#[tokio::test]
#[ignore]
async fn interactive_upower_test() {
    let mut connection_factory = ConnectionFactory::new();
    let mut receive_channel = UPowerSensor::new(connection_factory.get_system().await.unwrap())
        .await
        .unwrap();
    assert_eq!(*receive_channel.borrow_and_update(), PowerStatus::External);
    println!("Please disconnect the external power source");
    receive_channel.changed().await.unwrap();
    match *receive_channel.borrow_and_update() {
        PowerStatus::Battery(_) => {}
        PowerStatus::External => panic!("Expected a message the computer running from battery"),
    }
    println!("Please connect the external power source");
    receive_channel.changed().await.unwrap();
    assert_eq!(*receive_channel.borrow_and_update(), PowerStatus::External);
}
