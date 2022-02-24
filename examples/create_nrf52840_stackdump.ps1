# Go to the application
Set-Location -Path ./nrf52840
# Remove the old logs
Remove-Item -Path logs -Force -Recurse
# Run embed (compile, flash & RTT logs)
cargo embed --release
# Go back
Set-Location -Path ../
# Copy the binary to the data folder
Copy-Item ./nrf52840/target/thumbv7em-none-eabihf/release/nrf52840 ./data/nrf52840
# Copy the dump data log to the data folder
Copy-Item (Get-ChildItem ./nrf52840/logs *.dat) ./data/nrf52840.dump
