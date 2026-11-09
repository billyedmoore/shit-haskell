data X = X
instance Show X where
  show X = "x"
  bogus X = 1

main :: IO ()
main = print X
