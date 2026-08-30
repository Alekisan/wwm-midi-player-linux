//! Qt6/QML front-end entry point.

pub mod bridge;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};

fn main() {
    // Use the KDE desktop style so the app follows the user's Breeze theme.
    // Falls back to Fusion where qqc2-desktop-style isn't installed.
    QQuickStyle::set_style(&QString::from("org.kde.desktop"));
    QQuickStyle::set_fallback_style(&QString::from("Fusion"));

    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(mut app) = app.as_mut() {
        app.as_mut()
            .set_application_name(&QString::from("Where Winds Meet MIDI Player"));
        app.set_organization_name(&QString::from("wwm"));
    }

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/com/wwm/player/qml/main.qml"));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
