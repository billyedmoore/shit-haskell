{-# LANGUAGE NoExistentialQuantification, NoGADTs #-}

data T a = Eq a => Mk a

main :: IO ()
main = pure ()
