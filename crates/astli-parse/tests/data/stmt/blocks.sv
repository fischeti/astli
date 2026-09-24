initial begin
  // A block carries its labels at both ends.
  begin : b x = 1; end : b
  fork x = 1; join
  fork x = 1; join_any
  fork x = 1; join_none
end
