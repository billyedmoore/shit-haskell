{-# LANGUAGE GADTs, NoExplicitForAll #-}

data T a where
  Mk :: forall b. { unMk :: b } -> T b

main :: IO ()
main = pure ()
