-- A pattern synonym from another module whose definition binds `m` (time's YearMonth, through a
-- view pattern), used where the program's own `m` is in scope.
import Data.Time.Calendar.Month

check :: Month -> Bool
check m = case m of
  YearMonth y my -> m == YearMonth y my

main :: IO ()
main = do
  print (map check [YearMonth 2024 3, YearMonth 1999 12])
  print ((\m -> case m of YearMonth y my -> (y, my, m == YearMonth y my)) (YearMonth 2001 7))
