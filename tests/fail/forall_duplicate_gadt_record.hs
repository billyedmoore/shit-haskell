{-# LANGUAGE GADTs #-}

data D where
  D :: forall a a. { field :: a } -> D

main :: IO ()
main = pure ()
