{-# LANGUAGE TypeFamilies #-}

type family Pick a
type family Result a
type instance Result (Maybe (Pick a)) = Bool

main = pure ()
