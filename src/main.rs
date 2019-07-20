use std::io;

mod state;

fn main() {
    match state::State::from_file("rawrscope_proj.yml") {
        Ok(state) => println!("{:?}", state),
        Err(state::Error::OpenError { ref source, .. })
            if source.kind() == io::ErrorKind::NotFound =>
        {
            println!("project not found, writing default...");
            let state = state::State::default();
            if let Err(e) = state.write("rawrscope_proj.yml") {
                println!("{}", e);
            }
        }
        Err(e) => println!("{}", e),
    }
}
