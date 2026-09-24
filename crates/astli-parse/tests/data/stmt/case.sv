initial begin
  case (s) 0, 1: x = 1; default: x = 2; endcase
  priority case (a) 1: x = 1; endcase
end
