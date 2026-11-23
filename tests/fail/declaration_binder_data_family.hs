{-# LANGUAGE TypeFamilies #-}
data family Broken a a
main :: IO ()
main = pure ()
