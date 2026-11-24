{-# LANGUAGE ExistentialQuantification #-}
data T a = forall b. T a missing
main :: IO ()
main = pure ()
