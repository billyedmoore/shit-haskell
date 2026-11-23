{-# LANGUAGE GADTs, RankNTypes #-}
data T where
  T :: forall a. (forall b. b -> a) -> T
main :: IO ()
main = pure ()
