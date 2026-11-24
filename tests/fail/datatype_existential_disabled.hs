{-# LANGUAGE NoExistentialQuantification, NoGADTs #-}

data T = forall a. Mk a

main :: IO ()
main = pure ()
