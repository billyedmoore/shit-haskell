{-# LANGUAGE NoExplicitForAll #-}

class Example a
instance forall a. Example [a]

main :: IO ()
main = pure ()
