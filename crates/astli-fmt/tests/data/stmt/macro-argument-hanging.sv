// An argument's later line that hangs left of where the argument starts
// moves with the indentation of its line, but no further right than the
// argument's start: right of it, the next pass would read the line as aligned
// under the argument and move it with that instead.
task t();
  begin
    begin
      begin
        begin
          `uvm_info(`gfn, $sformatf({"filtered item #%0d on the A channel has been forwarded to the",
                    " scoreboard, to be compared with the item it expects"}, cnt), UVM_LOW)
        end
      end
    end
  end
endtask
