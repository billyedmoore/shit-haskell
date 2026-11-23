{-# LANGUAGE GADTs, ScopedTypeVariables #-}
data T a where
  T :: forall b. a -> b -> T a
main :: IO ()
main = pure ()
