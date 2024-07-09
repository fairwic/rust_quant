# Use an official Rust image as the base image
FROM rust:latest

# Set the working directory inside the container
WORKDIR /usr/src/rust_quant

# Copy the current directory contents into the container at /usr/src/rust_quant
COPY . .

# Install necessary runtime dependencies

# The command to build and run the Rust application
CMD ["sh", "-c", "cargo build --release && ./target/release/rust-quant"]