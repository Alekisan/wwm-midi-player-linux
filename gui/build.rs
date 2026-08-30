use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("com.wwm.player").qml_file("qml/main.qml"))
        .file("src/bridge.rs")
        .qt_module("Quick")
        .build();
}
