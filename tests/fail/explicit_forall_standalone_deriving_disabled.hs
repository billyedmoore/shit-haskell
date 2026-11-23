{-# LANGUAGE StandaloneDeriving, NoExplicitForAll #-}

data Wrap a = Wrap a
deriving instance forall a. Eq a => Eq (Wrap a)

main :: IO ()
main = pure ()
