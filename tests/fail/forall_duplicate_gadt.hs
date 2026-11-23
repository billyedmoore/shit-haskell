{-# LANGUAGE GADTs #-}

data D where
  D :: forall a a. a -> D

main :: IO ()
main = pure ()
