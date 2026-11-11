-- Big Integer arithmetic: factorial and the digit sum.
main :: IO ()
main = do
  let f = product [1 .. 3000 :: Integer]
  print (sum (map (\c -> fromEnum c - 48) (show f)), length (show f))
