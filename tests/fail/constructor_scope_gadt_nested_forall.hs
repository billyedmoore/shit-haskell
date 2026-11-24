{-# LANGUAGE GADTs #-}
data T where
  T :: forall a. forall b. a -> b -> T
main :: IO ()
main = pure ()
