use std::io;

mod state;

fn main() {
    match state::State::from_file("test.rprj") {
        Ok(state) => println!("{:?}", state),
        Err(state::ReadError::OpenError { ref source, .. })
            if source.kind() == io::ErrorKind::NotFound =>
        {
            println!("project not found, writing default...");
            let state = state::State::default();
            if let Err(e) = state.write("test.rprj") {
                println!("{}", e);
            }
        }
        Err(e) => println!("{}", e),
    }
}
