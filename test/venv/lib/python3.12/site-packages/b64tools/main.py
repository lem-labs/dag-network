import base64

def b32encode(data: str) -> str:
    """
    Encode a string to base32.
    
    Args:
        data (str): The string to encode.
        
    Returns:
        str: The base32 encoded string.
    """
    return base64.b32encode(data.encode()).decode()

def b32decode(data: str) -> str:
    """
    Decode a base32 encoded string.
    
    Args:
        data (str): The base32 encoded string to decode.
        
    Returns:
        str: The decoded string.
    """
    return base64.b32decode(data.encode()).decode()

def b16encode(data: str) -> str:
    """
    Encode a string to base16.
    
    Args:
        data (str): The string to encode.
        
    Returns:
        str: The base16 encoded string.
    """
    return base64.b16encode(data.encode()).decode()
def b16decode(data: str) -> str:
    """
    Decode a base16 encoded string.
    
    Args:
        data (str): The base16 encoded string to decode.
        
    Returns:
        str: The decoded string.
    """
    return base64.b16decode(data.encode()).decode()

def b85encode(data: str) -> str:
    """
    Encode a string to base85.
    
    Args:
        data (str): The string to encode.
        
    Returns:
        str: The base85 encoded string.
    """
    return base64.b85encode(data.encode()).decode()

def b85decode(data: str) -> str:
    """
    Decode a base85 encoded string.
    
    Args:
        data (str): The base85 encoded string to decode.
        
    Returns:
        str: The decoded string.
    """
    return base64.b85decode(data.encode()).decode()

def b32hexencode(data: str) -> str:
    """
    Encode a string to base32hex.
    
    Args:
        data (str): The string to encode.
        
    Returns:
        str: The base32hex encoded string.
    """
    return base64.b32hexencode(data.encode()).decode()

def b32hexdecode(data: str) -> str:
    """
    Decode a base32hex encoded string.
    
    Args:
        data (str): The base32hex encoded string to decode.
        
    Returns:
        str: The decoded string.
    """
    return base64.b32hexdecode(data.encode()).decode()

def b64encode(data: str) -> str:
    """
    Encode a string to base64.
    
    Args:
        data (str): The string to encode.
        
    Returns:
        str: The base64 encoded string.
    """
    return base64.b64encode(data.encode()).decode()

def b64decode(data: str) -> str:
    """
    Decode a base64 encoded string.
    
    Args:
        data (str): The base64 encoded string to decode.
        
    Returns:
        str: The decoded string.
    """
    return base64.b64decode(data.encode()).decode()
