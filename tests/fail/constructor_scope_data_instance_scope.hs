{-# LANGUAGE TypeFamilies, GADTs #-}
data family Family a
data instance Family a where
  T :: forall b. a -> b -> Family a
main :: IO ()
main = pure ()
