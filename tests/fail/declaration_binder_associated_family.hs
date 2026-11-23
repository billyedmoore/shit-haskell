{-# LANGUAGE TypeFamilies #-}
class Owner a where
  type Family a a
main :: IO ()
main = pure ()
