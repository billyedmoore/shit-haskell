data T = T

instance Show T where
  show _ = "first"

instance Show T where
  show _ = "second"

main = pure ()
