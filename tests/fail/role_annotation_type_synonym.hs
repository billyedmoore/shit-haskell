{-# LANGUAGE RoleAnnotations #-}

type role Box representational
type Box a = Maybe a

main = pure ()
